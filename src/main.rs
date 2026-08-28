#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use disk_project_organizer::{app::OrganizerApp, projects, scanner};
use serde_json::json;
use std::{env, fs, path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let arguments: Vec<String> = env::args().collect();
    if arguments.get(1).is_some_and(|argument| argument == "--self-test") {
        let output = arguments
            .get(2)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("self-test-output.json"));
        run_self_test(&output)?;
        return Ok(());
    }
    if arguments.get(1).is_some_and(|argument| argument == "--benchmark") {
        let output = arguments
            .get(2)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("benchmark-output.json"));
        run_benchmark(&output)?;
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([980.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "磁盘工程整理助手",
        options,
        Box::new(|context| Ok(Box::new(OrganizerApp::new(context)))),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn run_benchmark(output: &PathBuf) -> Result<()> {
    let root = env::temp_dir().join(format!("disk-project-organizer-benchmark-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root)?;
    let file_total = 5_000_u64;
    for index in 0..file_total {
        let directory = root.join(format!("bucket-{:03}", index % 100));
        fs::create_dir_all(&directory)?;
        let payload = format!("unique-{index:08}-{}", "x".repeat(96));
        fs::write(directory.join(format!("file-{index:05}.dat")), payload)?;
    }
    fs::write(root.join("duplicate-a.bin"), b"benchmark-duplicate-content")?;
    fs::write(root.join("duplicate-b.bin"), b"benchmark-duplicate-content")?;
    let project = root.join("benchmark_project");
    fs::create_dir_all(&project)?;
    fs::write(project.join("CMakeLists.txt"), "project(BenchmarkProject)\n")?;
    fs::write(project.join("main.cpp"), "int main(){return 0;}\n")?;

    let cancel = scanner::CancelToken::default();
    let large_started = Instant::now();
    let large = scanner::scan_large_files(std::slice::from_ref(&root), 1, &cancel, None)?;
    let large_elapsed = large_started.elapsed();
    let duplicate_started = Instant::now();
    let duplicates = scanner::scan_duplicates(std::slice::from_ref(&root), 1, &cancel, None)?;
    let duplicate_elapsed = duplicate_started.elapsed();
    let project_started = Instant::now();
    let projects = projects::scan_projects(std::slice::from_ref(&root), &cancel, None)?;
    let project_elapsed = project_started.elapsed();

    let payload = json!({
        "ok": !large.is_empty() && !duplicates.is_empty() && !projects.is_empty(),
        "generated_files": file_total + 4,
        "large_scan_ms": large_elapsed.as_millis(),
        "large_scan_files_per_second": (file_total as f64 / large_elapsed.as_secs_f64()).round(),
        "duplicate_scan_ms": duplicate_elapsed.as_millis(),
        "duplicate_scan_files_per_second": (file_total as f64 / duplicate_elapsed.as_secs_f64()).round(),
        "project_scan_ms": project_elapsed.as_millis(),
        "large_results": large.len(),
        "duplicate_groups": duplicates.len(),
        "projects": projects.len(),
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_vec_pretty(&payload)?)?;
    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn run_self_test(output: &PathBuf) -> Result<()> {
    let root = env::temp_dir().join(format!("disk-project-organizer-self-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root)?;
    fs::write(root.join("duplicate-a.bin"), b"same-content")?;
    fs::write(root.join("duplicate-b.bin"), b"same-content")?;
    fs::write(root.join("large.bin"), vec![b'x'; 4096])?;
    let project = root.join("demo_project");
    fs::create_dir_all(&project)?;
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"demo-tool\"\ndescription = \"demo project\"\n",
    )?;
    fs::write(project.join("main.py"), "print('ok')\n")?;
    let cancel = scanner::CancelToken::default();
    let large = scanner::scan_large_files(std::slice::from_ref(&root), 1024, &cancel, None)?;
    let duplicates = scanner::scan_duplicates(std::slice::from_ref(&root), 1, &cancel, None)?;
    let projects = projects::scan_projects(std::slice::from_ref(&root), &cancel, None)?;
    let payload = json!({
        "ok": !large.is_empty() && !duplicates.is_empty() && !projects.is_empty(),
        "large_files": large.len(),
        "duplicate_groups": duplicates.len(),
        "projects": projects.len(),
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_vec_pretty(&payload)?).context("write self-test output")?;
    let _ = fs::remove_dir_all(root);
    Ok(())
}
