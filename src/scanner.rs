use crate::{
    model::{DuplicateGroup, LargeFile, ProgressUpdate},
    util::{available_threads, default_skip_names, system_time_to_unix},
};
use anyhow::{Context, Result, anyhow};
use blake3::Hasher as Blake3Hasher;
use dua_core::{Options, Order, walk};
use parking_lot::Mutex;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub type ProgressFn = Arc<dyn Fn(ProgressUpdate) + Send + Sync>;

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(anyhow!("cancelled"))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug)]
struct FileRecord {
    path: PathBuf,
    size: u64,
    allocated: u64,
    modified_unix: u64,
}

fn emit(progress: &Option<ProgressFn>, stage: &str, message: String, current: u64, total: u64) {
    if let Some(callback) = progress {
        callback(ProgressUpdate {
            stage: stage.to_owned(),
            message,
            current,
            total,
        });
    }
}

fn collect_files(
    roots: &[PathBuf],
    min_size: u64,
    cancel: &CancelToken,
    progress: &Option<ProgressFn>,
) -> Result<Vec<FileRecord>> {
    let skip_names = Arc::new(default_skip_names());
    let records = Mutex::new(Vec::new());
    let file_count = AtomicU64::new(0);
    let mut hard_links = HashSet::new();

    for root in roots {
        cancel.check()?;
        let skip = Arc::clone(&skip_names);
        let cancel_for_walk = cancel.clone();
        let descend = move |entry: &dua_core::Entry| {
            !cancel_for_walk.is_cancelled()
                && entry.file_type.is_dir()
                && !skip.contains(&entry.file_name.to_string_lossy().to_lowercase())
        };
        for item in walk(
            root,
            available_threads(),
            Order::Completion,
            Options::default(),
            descend,
        ) {
            cancel.check()?;
            let Ok(entry) = item else { continue };
            if !entry.file_type.is_file() {
                continue;
            }
            let Ok(metadata) = entry.metadata else { continue };
            if metadata.len() < min_size {
                continue;
            }
            if let Some(link_id) = metadata.hard_link_id()
                && !hard_links.insert(link_id)
            {
                continue;
            }
            records.lock().push(FileRecord {
                path: entry.path(),
                size: metadata.len(),
                allocated: metadata.allocated_size(),
                modified_unix: metadata.modified().map_or(0, system_time_to_unix),
            });
            let current = file_count.fetch_add(1, Ordering::Relaxed) + 1;
            if current.is_multiple_of(2_000) {
                emit(
                    progress,
                    "walk",
                    format!("已收集 {current} 个候选文件：{}", root.display()),
                    current,
                    0,
                );
            }
        }
    }
    Ok(records.into_inner())
}

pub fn scan_large_files(
    roots: &[PathBuf],
    min_size: u64,
    cancel: &CancelToken,
    progress: Option<ProgressFn>,
) -> Result<Vec<LargeFile>> {
    emit(&progress, "large", "开始并行扫描大文件".to_owned(), 0, 0);
    let mut files: Vec<_> = collect_files(roots, min_size, cancel, &progress)?
        .into_iter()
        .map(|file| LargeFile {
            path: file.path,
            size_bytes: file.size,
            allocated_bytes: file.allocated,
            modified_unix: file.modified_unix,
        })
        .collect();
    files.par_sort_unstable_by(|left, right| right.size_bytes.cmp(&left.size_bytes));
    emit(
        &progress,
        "large",
        format!("完成：发现 {} 个大文件", files.len()),
        files.len() as u64,
        files.len() as u64,
    );
    Ok(files)
}

fn quick_fingerprint(path: &Path, size: u64, cancel: &CancelToken) -> Result<blake3::Hash> {
    cancel.check()?;
    const SAMPLE: usize = 64 * 1024;
    let mut file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut hasher = Blake3Hasher::new();
    hasher.update(&size.to_le_bytes());
    let mut first = vec![0_u8; SAMPLE.min(size as usize)];
    file.read_exact(&mut first)?;
    hasher.update(&first);
    if size > SAMPLE as u64 {
        file.seek(SeekFrom::End(-(SAMPLE.min(size as usize) as i64)))?;
        let mut last = vec![0_u8; SAMPLE.min(size as usize)];
        file.read_exact(&mut last)?;
        hasher.update(&last);
    }
    Ok(hasher.finalize())
}

fn full_sha256(path: &Path, cancel: &CancelToken) -> Result<String> {
    cancel.check()?;
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 4 * 1024 * 1024];
    loop {
        cancel.check()?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect())
}

pub fn scan_duplicates(
    roots: &[PathBuf],
    min_size: u64,
    cancel: &CancelToken,
    progress: Option<ProgressFn>,
) -> Result<Vec<DuplicateGroup>> {
    emit(
        &progress,
        "duplicate",
        "阶段 1/3：按文件大小分组".to_owned(),
        0,
        0,
    );
    let files = collect_files(roots, min_size, cancel, &progress)?;
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for file in files {
        by_size.entry(file.size).or_default().push(file.path);
    }
    let size_candidates: Vec<_> = by_size.into_iter().filter(|(_, paths)| paths.len() > 1).collect();
    let quick_total: usize = size_candidates.iter().map(|(_, paths)| paths.len()).sum();
    emit(
        &progress,
        "duplicate",
        format!("阶段 2/3：BLAKE3 首尾采样，共 {quick_total} 个候选"),
        0,
        quick_total as u64,
    );
    let quick_done = AtomicU64::new(0);
    let quick_rows: Vec<(u64, blake3::Hash, PathBuf)> = size_candidates
        .par_iter()
        .flat_map_iter(|(size, paths)| paths.iter().map(move |path| (*size, path)))
        .filter_map(|(size, path)| {
            let fingerprint = quick_fingerprint(path, size, cancel).ok()?;
            let current = quick_done.fetch_add(1, Ordering::Relaxed) + 1;
            if current.is_multiple_of(32) {
                emit(
                    &progress,
                    "duplicate",
                    format!("快速指纹 {current}/{quick_total}"),
                    current,
                    quick_total as u64,
                );
            }
            Some((size, fingerprint, path.clone()))
        })
        .collect();
    cancel.check()?;

    let mut by_quick: HashMap<(u64, blake3::Hash), Vec<PathBuf>> = HashMap::new();
    for (size, fingerprint, path) in quick_rows {
        by_quick.entry((size, fingerprint)).or_default().push(path);
    }
    let full_candidates: Vec<_> = by_quick
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .collect();
    let full_total: usize = full_candidates.iter().map(|(_, paths)| paths.len()).sum();
    emit(
        &progress,
        "duplicate",
        format!("阶段 3/3：完整 SHA-256，共 {full_total} 个候选"),
        0,
        full_total as u64,
    );
    let full_done = AtomicU64::new(0);
    let full_rows: Vec<(u64, String, PathBuf)> = full_candidates
        .par_iter()
        .flat_map_iter(|((size, _), paths)| paths.iter().map(move |path| (*size, path)))
        .filter_map(|(size, path)| {
            let digest = full_sha256(path, cancel).ok()?;
            let current = full_done.fetch_add(1, Ordering::Relaxed) + 1;
            emit(
                &progress,
                "duplicate",
                format!("SHA-256 {current}/{full_total}"),
                current,
                full_total as u64,
            );
            Some((size, digest, path.clone()))
        })
        .collect();
    cancel.check()?;

    let mut exact: HashMap<(u64, String), Vec<PathBuf>> = HashMap::new();
    for (size, digest, path) in full_rows {
        exact.entry((size, digest)).or_default().push(path);
    }
    let mut groups: Vec<_> = exact
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .enumerate()
        .map(|(index, ((size, digest), mut paths))| {
            paths.sort_unstable();
            DuplicateGroup {
                id: format!("DUP-{:04}", index + 1),
                sha256: digest,
                size_bytes: size,
                paths,
            }
        })
        .collect();
    groups.par_sort_unstable_by(|left, right| right.reclaimable_bytes().cmp(&left.reclaimable_bytes()));
    emit(
        &progress,
        "duplicate",
        format!("完成：发现 {} 组完全重复文件", groups.len()),
        groups.len() as u64,
        groups.len() as u64,
    );
    Ok(groups)
}
