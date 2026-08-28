use crate::model::OperationRecord;
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

pub const CATEGORIES: &[&str] = &[
    "00_Worktrees",
    "01_仿真建模",
    "02_机器人",
    "03_AI工具",
    "04_工业接口",
    "05_Cpp_Qt",
    "06_Python",
    "07_Web_Node",
    "08_多语言",
    "09_学习测试",
    "10_SDK依赖",
    "11_历史构建",
    "90_待确认",
];

#[must_use]
pub fn default_skip_names() -> HashSet<String> {
    [
        "$recycle.bin",
        "system volume information",
        "recovery",
        "windows",
        "program files",
        "program files (x86)",
        "programdata",
        "appdata",
        "node_modules",
        ".venv",
        "venv",
        "env",
        "__pycache__",
        ".cache",
        ".tox",
        ".mypy_cache",
        ".pytest_cache",
        ".ruff_cache",
        "site-packages",
        "cmakefiles",
        "_cpack_packages",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[must_use]
pub fn project_skip_names() -> HashSet<String> {
    let mut names = default_skip_names();
    for name in [".git", ".vs", ".idea", "build", "dist", "target"] {
        names.insert(name.to_owned());
    }
    names
}

#[must_use]
pub fn human_size(bytes: u64) -> String {
    let mut value = bytes as f64;
    for unit in ["B", "KB", "MB", "GB", "TB"] {
        if value < 1024.0 || unit == "TB" {
            return format!("{value:.2} {unit}");
        }
        value /= 1024.0;
    }
    format!("{bytes} B")
}

#[must_use]
pub fn system_time_to_unix(value: SystemTime) -> u64 {
    value
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[must_use]
pub fn format_unix(value: u64) -> String {
    let time = UNIX_EPOCH + std::time::Duration::from_secs(value);
    let local: DateTime<Local> = time.into();
    local.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[must_use]
pub fn available_threads() -> usize {
    std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get)
}

#[must_use]
pub fn sanitize_name(name: &str, fallback: &str) -> String {
    let mut output = String::with_capacity(name.len());
    let invalid = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
    let mut previous_underscore = false;
    for character in name.trim().trim_end_matches('.').chars() {
        let replacement = character.is_control() || invalid.contains(&character) || character.is_whitespace();
        if replacement {
            if !previous_underscore {
                output.push('_');
                previous_underscore = true;
            }
        } else {
            output.push(character);
            previous_underscore = character == '_';
        }
    }
    let cleaned = output.trim_matches('_');
    let value = if cleaned.is_empty() { fallback } else { cleaned };
    value.chars().take(60).collect()
}

#[must_use]
pub fn run_git(path: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("safe.directory=*")
        .arg("-C")
        .arg(path)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    match command.output() {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        _ => String::new(),
    }
}

pub fn export_serializable<T: serde::Serialize>(rows: &[T], path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let mut writer = csv::WriterBuilder::new().has_headers(true).from_path(path)?;
    for row in rows {
        writer.serialize(row)?;
    }
    writer.flush()?;
    Ok(())
}

pub fn audit_log_path() -> PathBuf {
    let root = dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("DiskProjectOrganizer");
    let _ = fs::create_dir_all(&root);
    root.join("operations.jsonl")
}

pub fn append_audit(record: &OperationRecord) -> Result<()> {
    let path = audit_log_path();
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    serde_json::to_writer(&mut file, record)?;
    writeln!(file)?;
    Ok(())
}

pub fn make_record(
    success: bool,
    action: &str,
    source: impl Into<String>,
    destination: impl Into<String>,
    message: impl Into<String>,
) -> OperationRecord {
    OperationRecord {
        success,
        action: action.to_owned(),
        source: source.into(),
        destination: destination.into(),
        message: message.into(),
        time: Local::now().to_rfc3339(),
    }
}
