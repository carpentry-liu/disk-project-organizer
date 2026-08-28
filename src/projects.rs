use crate::{
    model::{GitInfo, ProgressUpdate, ProjectInfo, ProjectPlan},
    scanner::{CancelToken, ProgressFn},
    util::{CATEGORIES, available_threads, project_skip_names, run_git, sanitize_name},
};
use anyhow::Result;
use dua_core::{Options, Order, walk};
use rayon::prelude::*;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

const MARKERS: &[&str] = &[
    "CMakeLists.txt",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "requirements.txt",
    "Pipfile",
    "poetry.lock",
    "uv.lock",
    "environment.yml",
    "package.json",
    "Cargo.toml",
    "go.mod",
    "Makefile",
];

fn emit(progress: &Option<ProgressFn>, message: String, current: u64, total: u64) {
    if let Some(callback) = progress {
        callback(ProgressUpdate {
            stage: "projects".to_owned(),
            message,
            current,
            total,
        });
    }
}

fn marker_list(path: &Path) -> Vec<String> {
    let mut markers: Vec<String> = MARKERS
        .iter()
        .filter(|marker| path.join(marker).is_file())
        .map(|marker| (*marker).to_owned())
        .collect();
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let extension = entry_path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if extension.eq_ignore_ascii_case("sln") && !markers.iter().any(|value| value == "*.sln") {
                markers.push("*.sln".to_owned());
            } else if extension.eq_ignore_ascii_case("vcxproj")
                && !markers.iter().any(|value| value == "*.vcxproj")
            {
                markers.push("*.vcxproj".to_owned());
            }
        }
    }
    markers.sort_unstable();
    markers
}

fn inspect_git(path: &Path) -> GitInfo {
    let marker = path.join(".git");
    let marker_kind = if marker.is_dir() {
        "directory"
    } else if marker.is_file() {
        "file"
    } else {
        return GitInfo::default();
    };
    let head = run_git(path, &["rev-parse", "HEAD"]);
    let mut branch = run_git(path, &["symbolic-ref", "--short", "-q", "HEAD"]);
    if !head.is_empty() && branch.is_empty() {
        branch = "DETACHED".to_owned();
    }
    let remote = run_git(path, &["remote", "get-url", "origin"]);
    let worktrees = run_git(path, &["worktree", "list", "--porcelain"]);
    let worktree_count = worktrees
        .lines()
        .filter(|line| line.starts_with("worktree "))
        .count();
    let tracked_dirty =
        !head.is_empty() && !run_git(path, &["status", "--porcelain=v1", "--untracked-files=no"]).is_empty();
    GitInfo {
        marker_kind: marker_kind.to_owned(),
        head: head.clone(),
        branch,
        remote,
        worktree_count,
        tracked_dirty,
        valid: !head.is_empty(),
    }
}

fn read_description(path: &Path) -> String {
    let package_json = path.join("package.json");
    if let Ok(text) = fs::read_to_string(&package_json)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
    {
        let name = value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let description = value
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let combined = [name, description]
            .into_iter()
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" — ");
        if !combined.is_empty() {
            return combined.chars().take(240).collect();
        }
    }

    let pyproject = path.join("pyproject.toml");
    if let Ok(text) = fs::read_to_string(&pyproject) {
        let name = extract_toml_value(&text, "name");
        let description = extract_toml_value(&text, "description");
        let combined = [name, description]
            .into_iter()
            .flatten()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" — ");
        if !combined.is_empty() {
            return combined.chars().take(240).collect();
        }
    }

    let cmake = path.join("CMakeLists.txt");
    if let Ok(text) = fs::read_to_string(cmake)
        && let Some(project) = extract_cmake_project(&text)
    {
        return format!("CMake 项目：{project}");
    }

    if let Ok(entries) = fs::read_dir(path) {
        let mut readmes: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|candidate| {
                candidate.is_file()
                    && candidate
                        .file_name()
                        .and_then(|value| value.to_str())
                        .is_some_and(|name| {
                            let lower = name.to_lowercase();
                            lower.starts_with("readme")
                                || name.starts_with("说明")
                                || name.starts_with("介绍")
                        })
            })
            .collect();
        readmes.sort_unstable();
        for readme in readmes.into_iter().take(2) {
            if let Ok(text) = fs::read_to_string(readme) {
                for line in text.lines().take(80) {
                    let clean = line
                        .trim_start_matches(['#', '>', '*', '-', '`', '_', ' '])
                        .trim();
                    if clean.chars().count() >= 4
                        && !clean.starts_with("![")
                        && !clean.starts_with("[![")
                        && !clean.starts_with("<img")
                    {
                        return clean.chars().take(240).collect();
                    }
                }
            }
        }
    }
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("未命名项目")
        .to_owned()
}

fn extract_toml_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        let prefix = format!("{key} =");
        line.strip_prefix(&prefix).and_then(|value| {
            let trimmed = value.trim().trim_matches(['"', '\'']);
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
    })
}

fn extract_cmake_project(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        let start = lower.find("project(")? + "project(".len();
        let remainder = &trimmed[start..];
        let name = remainder
            .split(|character: char| character.is_whitespace() || character == ')')
            .next()?;
        (!name.is_empty()).then(|| name.to_owned())
    })
}

fn detect_languages(path: &Path, markers: &[String], cancel: &CancelToken) -> Vec<String> {
    let mut languages = HashSet::new();
    let marker_set: HashSet<&str> = markers.iter().map(String::as_str).collect();
    if marker_set
        .iter()
        .any(|marker| matches!(*marker, "CMakeLists.txt" | "*.sln" | "*.vcxproj" | "Makefile"))
    {
        languages.insert("C/C++/Qt".to_owned());
    }
    if marker_set.iter().any(|marker| {
        matches!(
            *marker,
            "pyproject.toml"
                | "setup.py"
                | "setup.cfg"
                | "requirements.txt"
                | "Pipfile"
                | "poetry.lock"
                | "uv.lock"
                | "environment.yml"
        )
    }) {
        languages.insert("Python".to_owned());
    }
    if marker_set.contains("package.json") {
        languages.insert("JS/TS/Web".to_owned());
    }
    if marker_set.contains("Cargo.toml") {
        languages.insert("Rust".to_owned());
    }
    if marker_set.contains("go.mod") {
        languages.insert("Go".to_owned());
    }

    let skip = Arc::new(project_skip_names());
    let skip_for_walk = Arc::clone(&skip);
    let cancel_for_walk = cancel.clone();
    let descend = move |entry: &dua_core::Entry| {
        !cancel_for_walk.is_cancelled()
            && entry.file_type.is_dir()
            && !skip_for_walk.contains(&entry.file_name.to_string_lossy().to_lowercase())
    };
    let mut checked = 0_u64;
    for item in walk(path, 2, Order::Completion, Options::default(), descend) {
        if checked >= 8_000 || cancel.is_cancelled() {
            break;
        }
        let Ok(entry) = item else { continue };
        if !entry.file_type.is_file() {
            continue;
        }
        checked += 1;
        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_lowercase();
        let language = match extension.as_str() {
            "c" | "cc" | "cpp" | "cxx" | "h" | "hpp" | "hxx" | "qml" | "ui" => "C/C++/Qt",
            "py" | "pyw" | "ipynb" => "Python",
            "js" | "jsx" | "ts" | "tsx" | "vue" | "svelte" => "JS/TS/Web",
            "jl" => "Julia",
            "mo" => "Modelica",
            "java" | "kt" => "Java/Kotlin",
            "cs" => "C#",
            "rs" => "Rust",
            "go" => "Go",
            "m" => "MATLAB",
            _ => continue,
        };
        languages.insert(language.to_owned());
    }
    let mut output: Vec<String> = languages.into_iter().collect();
    output.sort_unstable();
    if output.is_empty() {
        output.push("待确认".to_owned());
    }
    output
}

fn classify(name: &str, description: &str, languages: &[String], path: &Path) -> String {
    let text = format!("{name} {description} {}", path.display()).to_lowercase();
    let rules = [
        (
            "11_历史构建",
            [
                "before", "backup", "_last", "release", "build-", "历史", "旧版", "归档",
            ]
            .as_slice(),
        ),
        (
            "10_SDK依赖",
            [
                "opencrg",
                "vcpkg",
                "sdk",
                "toolchain",
                "libraries",
                "boost",
                "third-party",
            ]
            .as_slice(),
        ),
        (
            "01_仿真建模",
            [
                "mworks",
                "sysplorer",
                "syslab",
                "modelica",
                "mols",
                "netlist",
                "risa",
                "vpa",
                "hil",
                "carla",
                "road",
                "weather",
                "mbse",
                "fmu",
                "仿真",
            ]
            .as_slice(),
        ),
        (
            "02_机器人",
            [
                "robot",
                "sys3d",
                "sensor",
                "camera",
                "kuka",
                "topstar",
                "tuosida",
                "机器人",
                "传感器",
            ]
            .as_slice(),
        ),
        (
            "03_AI工具",
            [
                "claude", "codex", "opencode", "agent", "mcp", "skill", "gitnexus", " ai ", "智能",
            ]
            .as_slice(),
        ),
        (
            "04_工业接口",
            [
                "plc",
                "iotdb",
                "dataexchange",
                "data-exchange",
                "payload",
                "control",
                "工业",
                "接口",
            ]
            .as_slice(),
        ),
        (
            "09_学习测试",
            [
                "learning", "demo", "test", "poc", "study", "example", "学习", "示例", "测试",
            ]
            .as_slice(),
        ),
    ];
    for (category, keywords) in rules {
        if keywords.iter().any(|keyword| text.contains(keyword)) {
            return category.to_owned();
        }
    }
    match languages {
        [only] if only == "C/C++/Qt" => "05_Cpp_Qt".to_owned(),
        [only] if only == "Python" => "06_Python".to_owned(),
        [only] if only == "JS/TS/Web" => "07_Web_Node".to_owned(),
        [only] if only == "待确认" => "90_待确认".to_owned(),
        _ => "08_多语言".to_owned(),
    }
}

fn suggest_name(original: &str, description: &str, category: &str, languages: &[String]) -> String {
    const GENERIC: &[&str] = &[
        "ai",
        "app",
        "code",
        "cpp",
        "demo",
        "jl",
        "jh",
        "cxn",
        "mbd",
        "poc",
        "qt",
        "tool",
        "source",
        "model",
        "python",
        "julia",
        "route",
        "skills",
        "setting",
        "release",
        "新建文件夹",
    ];
    if !GENERIC.iter().any(|value| original.eq_ignore_ascii_case(value))
        && !original.starts_with("新建文件夹")
    {
        return sanitize_name(original, "项目");
    }
    let description = description
        .strip_prefix("CMake 项目：")
        .unwrap_or(description)
        .trim();
    if !description.is_empty() && !description.eq_ignore_ascii_case(original) {
        let first = description
            .split([
                '—', '|', ',', '，', '。', ';', '；', ':', '/', '\\', '[', ']', '(', ')',
            ])
            .next()
            .unwrap_or(description)
            .trim();
        if !first.is_empty() {
            return sanitize_name(first, "项目");
        }
    }
    let language = languages.first().map_or("项目", String::as_str).replace('/', "_");
    sanitize_name(
        &format!(
            "{}_{}_{}",
            category.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_'),
            language,
            original
        ),
        "项目",
    )
}

pub fn scan_projects(
    roots: &[PathBuf],
    cancel: &CancelToken,
    progress: Option<ProgressFn>,
) -> Result<Vec<ProjectInfo>> {
    let skip = Arc::new(project_skip_names());
    let candidates = parking_lot::Mutex::new(Vec::<PathBuf>::new());
    let visited = AtomicU64::new(0);
    for root in roots {
        cancel.check()?;
        let skip_for_walk = Arc::clone(&skip);
        let cancel_for_walk = cancel.clone();
        let descend = move |entry: &dua_core::Entry| {
            !cancel_for_walk.is_cancelled()
                && entry.file_type.is_dir()
                && !skip_for_walk.contains(&entry.file_name.to_string_lossy().to_lowercase())
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
            if !entry.file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            let markers = marker_list(&path);
            if path.join(".git").exists() || !markers.is_empty() {
                candidates.lock().push(path);
            }
            let count = visited.fetch_add(1, Ordering::Relaxed) + 1;
            if count.is_multiple_of(250) {
                emit(&progress, format!("已扫描 {count} 个目录"), count, 0);
            }
        }
    }

    let mut candidate_paths = candidates.into_inner();
    candidate_paths.sort_unstable_by_key(|path| path.components().count());
    candidate_paths.dedup();
    let mut roots_only = Vec::<PathBuf>::new();
    'candidate: for candidate in candidate_paths {
        for parent in &roots_only {
            if candidate.starts_with(parent) {
                continue 'candidate;
            }
        }
        roots_only.push(candidate);
    }
    let total = roots_only.len() as u64;
    let inspected = AtomicU64::new(0);
    let mut projects: Vec<ProjectInfo> = roots_only
        .par_iter()
        .map(|path| {
            let markers = marker_list(path);
            let git = inspect_git(path);
            let description = read_description(path);
            let languages = detect_languages(path, &markers, cancel);
            let mut category = classify(
                path.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default(),
                &description,
                &languages,
                path,
            );
            let (safe_to_move, safety_reason) = if git.marker_kind == "file" {
                category = "00_Worktrees".to_owned();
                (false, "链接式 Git worktree/子模块，默认不移动".to_owned())
            } else if git.worktree_count > 1 {
                category = "00_Worktrees".to_owned();
                (
                    false,
                    format!("关联 {} 个 Git worktree，默认不移动", git.worktree_count),
                )
            } else {
                (true, String::new())
            };
            let original_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("项目")
                .to_owned();
            let suggested_name = suggest_name(&original_name, &description, &category, &languages);
            let current = inspected.fetch_add(1, Ordering::Relaxed) + 1;
            emit(
                &progress,
                format!("分析项目 {current}/{total}：{original_name}"),
                current,
                total,
            );
            ProjectInfo {
                path: path.clone(),
                original_name,
                suggested_name,
                description,
                languages,
                category,
                markers,
                git,
                safe_to_move,
                safety_reason,
            }
        })
        .collect();
    projects.par_sort_unstable_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then_with(|| left.suggested_name.cmp(&right.suggested_name))
    });
    emit(
        &progress,
        format!("完成：发现 {} 个项目根", projects.len()),
        total,
        total,
    );
    Ok(projects)
}

#[must_use]
pub fn build_plans(projects: &[ProjectInfo], library_root: &Path) -> Vec<ProjectPlan> {
    let mut destinations = HashSet::new();
    projects
        .iter()
        .map(|project| {
            let base_name = sanitize_name(&project.suggested_name, &project.original_name);
            let mut destination = library_root.join(&project.category).join(&base_name);
            let mut suffix = 2;
            while destination.exists() || !destinations.insert(destination.clone()) {
                destination = library_root
                    .join(&project.category)
                    .join(format!("{base_name}_{suffix}"));
                suffix += 1;
            }
            ProjectPlan {
                source: project.path.clone(),
                destination: destination.clone(),
                name: destination
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&base_name)
                    .to_owned(),
                category: project.category.clone(),
                description: project.description.clone(),
                languages: project.languages.clone(),
                safe_to_move: project.safe_to_move,
                safety_reason: project.safety_reason.clone(),
                expected_head: project.git.head.clone(),
                expected_branch: project.git.branch.clone(),
                expected_remote: project.git.remote.clone(),
            }
        })
        .collect()
}

#[must_use]
pub fn categories() -> &'static [&'static str] {
    CATEGORIES
}
