use disk_project_organizer::{model, operations, projects, scanner, util};
use std::{collections::HashSet, fs, path::PathBuf};
use tempfile::tempdir;

#[test]
fn finds_large_and_duplicate_files() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("a.bin"), b"identical-content").unwrap();
    fs::write(directory.path().join("b.bin"), b"identical-content").unwrap();
    fs::write(directory.path().join("large.bin"), vec![0_u8; 4096]).unwrap();
    let cancel = scanner::CancelToken::default();
    let roots = vec![directory.path().to_path_buf()];
    let large = scanner::scan_large_files(&roots, 1024, &cancel, None).unwrap();
    let duplicates = scanner::scan_duplicates(&roots, 1, &cancel, None).unwrap();
    assert_eq!(large.len(), 1);
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].paths.len(), 2);
}

#[test]
fn discovers_python_project_and_builds_plan() {
    let directory = tempdir().unwrap();
    let project = directory.path().join("app");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"calibration-tool\"\ndescription = \"MBSE calibration tool\"\n",
    )
    .unwrap();
    fs::write(project.join("main.py"), "print('ok')\n").unwrap();
    let cancel = scanner::CancelToken::default();
    let projects = projects::scan_projects(&[directory.path().to_path_buf()], &cancel, None).unwrap();
    assert_eq!(projects.len(), 1);
    assert!(projects[0].languages.iter().any(|language| language == "Python"));
    let plans = projects::build_plans(&projects, &directory.path().join("library"));
    assert_eq!(plans.len(), 1);
    assert!(plans[0].destination.starts_with(directory.path().join("library")));
}

#[test]
fn refuses_to_recycle_every_copy() {
    let directory = tempdir().unwrap();
    let a = directory.path().join("a.bin");
    let b = directory.path().join("b.bin");
    fs::write(&a, b"same").unwrap();
    fs::write(&b, b"same").unwrap();
    let group = model::DuplicateGroup {
        id: "DUP-0001".to_owned(),
        sha256: "HASH".to_owned(),
        size_bytes: 4,
        paths: vec![a.clone(), b.clone()],
    };
    let selected: HashSet<PathBuf> = [a, b].into_iter().collect();
    let result = operations::recycle_duplicates(&[group], &selected);
    assert!(!result.success);
}

#[test]
fn safe_name_removes_windows_invalid_characters() {
    assert_eq!(util::sanitize_name("a:b/c*?", "project"), "a_b_c");
}

#[test]
fn moves_project_root_as_one_unit() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source");
    let destination = directory.path().join("library").join("05_Cpp_Qt").join("demo");
    fs::create_dir_all(source.join(".git")).unwrap();
    fs::write(source.join("CMakeLists.txt"), "project(Demo)\n").unwrap();
    fs::write(source.join("main.cpp"), "int main(){return 0;}\n").unwrap();
    let plan = model::ProjectPlan {
        source: source.clone(),
        destination: destination.clone(),
        name: "demo".to_owned(),
        category: "05_Cpp_Qt".to_owned(),
        description: "demo".to_owned(),
        languages: vec!["C/C++/Qt".to_owned()],
        safe_to_move: true,
        safety_reason: String::new(),
        expected_head: String::new(),
        expected_branch: String::new(),
        expected_remote: String::new(),
    };
    let result = operations::move_project(&plan, false);
    assert!(result.success, "{}", result.message);
    assert!(!source.exists());
    assert!(destination.join(".git").is_dir());
    assert!(destination.join("main.cpp").is_file());
}

#[test]
fn marks_git_file_worktree_as_unsafe() {
    let directory = tempdir().unwrap();
    let project = directory.path().join("linked-worktree");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join(".git"), "gitdir: C:/missing/worktrees/demo\n").unwrap();
    fs::write(project.join("pyproject.toml"), "[project]\nname='linked'\n").unwrap();
    let cancel = scanner::CancelToken::default();
    let projects = projects::scan_projects(&[directory.path().to_path_buf()], &cancel, None).unwrap();
    assert_eq!(projects.len(), 1);
    assert!(!projects[0].safe_to_move);
    assert_eq!(projects[0].category, "00_Worktrees");
}
