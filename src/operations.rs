use crate::{
    model::{DuplicateGroup, OperationRecord, ProjectPlan},
    util::{append_audit, make_record, run_git},
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

fn drive_prefix(path: &Path) -> Option<String> {
    path.components().find_map(|component| match component {
        Component::Prefix(prefix) => normalize_prefix(prefix.kind()),
        _ => None,
    })
}

#[cfg(windows)]
fn normalize_prefix(prefix: std::path::Prefix<'_>) -> Option<String> {
    use std::path::Prefix;
    match prefix {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            Some((letter as char).to_ascii_lowercase().to_string())
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => Some(format!(
            "{}\\{}",
            server.to_string_lossy().to_lowercase(),
            share.to_string_lossy().to_lowercase()
        )),
        _ => None,
    }
}

#[cfg(not(windows))]
fn normalize_prefix(prefix: std::path::Prefix<'_>) -> Option<String> {
    Some(prefix.as_os_str().to_string_lossy().to_lowercase())
}

fn verify_git(plan: &ProjectPlan, destination: &Path) -> bool {
    if plan.expected_head.is_empty() {
        return true;
    }
    let head = run_git(destination, &["rev-parse", "HEAD"]);
    let mut branch = run_git(destination, &["symbolic-ref", "--short", "-q", "HEAD"]);
    if !head.is_empty() && branch.is_empty() {
        branch = "DETACHED".to_owned();
    }
    let remote = run_git(destination, &["remote", "get-url", "origin"]);
    head == plan.expected_head && branch == plan.expected_branch && remote == plan.expected_remote
}

pub fn move_project(plan: &ProjectPlan, allow_cross_drive: bool) -> OperationRecord {
    let source = match plan.source.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            return make_record(
                false,
                "move_project",
                plan.source.display().to_string(),
                plan.destination.display().to_string(),
                format!("源目录不存在：{error}"),
            );
        }
    };
    let destination = plan.destination.clone();
    if !plan.safe_to_move {
        return make_record(
            false,
            "move_project",
            source.display().to_string(),
            destination.display().to_string(),
            &plan.safety_reason,
        );
    }
    if destination.exists() {
        return make_record(
            false,
            "move_project",
            source.display().to_string(),
            destination.display().to_string(),
            "目标目录已存在",
        );
    }
    let same_drive = drive_prefix(&source) == drive_prefix(&destination);
    if !same_drive && !allow_cross_drive {
        return make_record(
            false,
            "move_project",
            source.display().to_string(),
            destination.display().to_string(),
            "默认禁止跨盘移动",
        );
    }
    if let Some(parent) = destination.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return make_record(
            false,
            "move_project",
            source.display().to_string(),
            destination.display().to_string(),
            format!("无法创建目标目录：{error}"),
        );
    }
    let move_result = if same_drive {
        fs::rename(&source, &destination)
    } else {
        move_cross_drive(&source, &destination)
    };
    if let Err(error) = move_result {
        return make_record(
            false,
            "move_project",
            source.display().to_string(),
            destination.display().to_string(),
            format!("移动失败：{error}"),
        );
    }
    if source.exists() || !destination.is_dir() {
        return make_record(
            false,
            "move_project",
            source.display().to_string(),
            destination.display().to_string(),
            "移动后的路径验证失败",
        );
    }
    if !verify_git(plan, &destination) {
        let rollback = if same_drive {
            fs::rename(&destination, &source)
        } else {
            move_cross_drive(&destination, &source)
        };
        let message = if rollback.is_ok() {
            "Git 校验失败，已回滚"
        } else {
            "Git 校验失败且回滚失败，请查看审计日志"
        };
        return make_record(
            false,
            "move_project",
            source.display().to_string(),
            destination.display().to_string(),
            message,
        );
    }
    make_record(
        true,
        "move_project",
        source.display().to_string(),
        destination.display().to_string(),
        "项目根目录已整体移动并完成 Git 校验",
    )
}

fn move_cross_drive(source: &Path, destination: &Path) -> std::io::Result<()> {
    copy_directory(source, destination)?;
    fs::remove_dir_all(source)
}

fn copy_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}

pub fn move_projects(
    plans: &[ProjectPlan],
    selected: &HashSet<usize>,
    allow_cross_drive: bool,
) -> Vec<OperationRecord> {
    plans
        .iter()
        .enumerate()
        .filter(|(index, _)| selected.contains(index))
        .map(|(_, plan)| {
            let record = move_project(plan, allow_cross_drive);
            let _ = append_audit(&record);
            record
        })
        .collect()
}

pub fn recycle_duplicates(groups: &[DuplicateGroup], selected_paths: &HashSet<PathBuf>) -> OperationRecord {
    let mut group_counts: HashMap<&str, (usize, usize)> = HashMap::new();
    for group in groups {
        let selected = group
            .paths
            .iter()
            .filter(|path| selected_paths.contains(*path))
            .count();
        group_counts.insert(group.sha256.as_str(), (group.paths.len(), selected));
    }
    if let Some((hash, _)) = group_counts
        .iter()
        .find(|(_, (total, selected))| *selected >= *total && *selected > 0)
    {
        return make_record(
            false,
            "recycle_duplicates",
            "",
            "",
            format!("重复组 {hash} 至少必须保留一份"),
        );
    }
    let paths: Vec<PathBuf> = selected_paths
        .iter()
        .filter(|path| path.is_file())
        .cloned()
        .collect();
    if paths.is_empty() {
        return make_record(false, "recycle_duplicates", "", "", "未选择重复文件");
    }
    let result = trash::delete_all(&paths);
    let record = match result {
        Ok(()) => make_record(
            true,
            "recycle_duplicates",
            paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" | "),
            "Windows Recycle Bin",
            format!("已将 {} 个文件移入回收站", paths.len()),
        ),
        Err(error) => make_record(
            false,
            "recycle_duplicates",
            paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" | "),
            "Windows Recycle Bin",
            format!("回收站操作失败：{error}"),
        ),
    };
    let _ = append_audit(&record);
    record
}
