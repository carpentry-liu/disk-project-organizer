use crate::{
    model::{DuplicateGroup, LargeFile, ProgressUpdate, ProjectInfo, ProjectPlan, WorkerEvent},
    operations::{move_projects, recycle_duplicates},
    projects::{build_plans, categories, scan_projects},
    scanner::{CancelToken, ProgressFn, scan_duplicates, scan_large_files},
    util::{audit_log_path, export_serializable, format_unix, human_size},
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    LargeFiles,
    Duplicates,
    Projects,
    Audit,
}

pub struct OrganizerApp {
    tab: Tab,
    roots: Vec<PathBuf>,
    root_text: String,
    library_root: String,
    large_threshold_gb: f64,
    duplicate_threshold_mb: f64,
    large_files: Vec<LargeFile>,
    duplicate_groups: Vec<DuplicateGroup>,
    projects: Vec<ProjectInfo>,
    project_plans: Vec<ProjectPlan>,
    selected_duplicates: HashSet<PathBuf>,
    selected_projects: HashSet<usize>,
    edit_project_index: Option<usize>,
    edit_project_name: String,
    edit_project_category: String,
    allow_cross_drive: bool,
    running: bool,
    cancel: Option<CancelToken>,
    progress: ProgressUpdate,
    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
    logs: VecDeque<String>,
    confirm_recycle: bool,
    confirm_move: bool,
    max_visible_rows: usize,
}

impl OrganizerApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        install_chinese_font(&context.egui_ctx);
        // Keep the native window readable even when Windows reports a mixed or
        // high-contrast theme to egui during startup.
        context.egui_ctx.set_visuals(egui::Visuals::light());
        let (tx, rx) = unbounded();
        Self {
            tab: Tab::LargeFiles,
            roots: Vec::new(),
            root_text: String::new(),
            library_root: String::new(),
            large_threshold_gb: 1.0,
            duplicate_threshold_mb: 10.0,
            large_files: Vec::new(),
            duplicate_groups: Vec::new(),
            projects: Vec::new(),
            project_plans: Vec::new(),
            selected_duplicates: HashSet::new(),
            selected_projects: HashSet::new(),
            edit_project_index: None,
            edit_project_name: String::new(),
            edit_project_category: String::new(),
            allow_cross_drive: false,
            running: false,
            cancel: None,
            progress: ProgressUpdate {
                stage: "idle".to_owned(),
                message: "请选择一个或多个扫描目录".to_owned(),
                current: 0,
                total: 0,
            },
            tx,
            rx,
            logs: VecDeque::new(),
            confirm_recycle: false,
            confirm_move: false,
            max_visible_rows: 2_000,
        }
    }

    fn add_log(&mut self, message: impl Into<String>) {
        let stamp = chrono::Local::now().format("%H:%M:%S");
        self.logs.push_front(format!("[{stamp}] {}", message.into()));
        while self.logs.len() > 500 {
            self.logs.pop_back();
        }
    }

    fn poll_worker_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                WorkerEvent::Progress(progress) => self.progress = progress,
                WorkerEvent::LargeFiles(files) => {
                    self.add_log(format!("大文件扫描完成：{} 项", files.len()));
                    self.large_files = files;
                    self.finish_task();
                }
                WorkerEvent::Duplicates(groups) => {
                    self.add_log(format!("重复扫描完成：{} 组", groups.len()));
                    self.duplicate_groups = groups;
                    self.selected_duplicates.clear();
                    self.finish_task();
                }
                WorkerEvent::Projects(projects) => {
                    self.add_log(format!("工程识别完成：{} 个", projects.len()));
                    self.projects = projects;
                    self.rebuild_project_plans();
                    self.finish_task();
                }
                WorkerEvent::ProjectOperations(records) => {
                    let successes = records.iter().filter(|record| record.success).count();
                    self.add_log(format!("工程整理：成功 {successes}/{}", records.len()));
                    for record in records {
                        self.add_log(format!("{}：{}", record.action, record.message));
                    }
                    self.finish_task();
                }
                WorkerEvent::RecycleResult(record) => {
                    self.add_log(record.message.clone());
                    if record.success {
                        self.selected_duplicates.clear();
                    }
                    self.finish_task();
                }
                WorkerEvent::Failed(message) => {
                    self.add_log(format!("失败：{message}"));
                    self.progress.message = format!("失败：{message}");
                    self.finish_task();
                }
                WorkerEvent::Cancelled => {
                    self.add_log("操作已取消");
                    self.progress.message = "操作已取消".to_owned();
                    self.finish_task();
                }
            }
        }
    }

    fn finish_task(&mut self) {
        self.running = false;
        self.cancel = None;
    }

    fn start_task<F>(&mut self, task: F)
    where
        F: FnOnce(Sender<WorkerEvent>, CancelToken, ProgressFn) + Send + 'static,
    {
        if self.running {
            return;
        }
        let token = CancelToken::default();
        let tx = self.tx.clone();
        let progress_tx = tx.clone();
        let progress: ProgressFn = Arc::new(move |update| {
            let _ = progress_tx.send(WorkerEvent::Progress(update));
        });
        self.running = true;
        self.cancel = Some(token.clone());
        thread::spawn(move || task(tx, token, progress));
    }

    fn current_roots(&self) -> Vec<PathBuf> {
        self.roots.iter().filter(|path| path.is_dir()).cloned().collect()
    }

    fn add_root(&mut self, path: PathBuf) {
        if path.is_dir() && !self.roots.contains(&path) {
            self.add_log(format!("添加扫描目录：{}", path.display()));
            self.roots.push(path);
        }
    }

    fn rebuild_project_plans(&mut self) {
        let root = PathBuf::from(self.library_root.trim());
        if self.projects.is_empty() || self.library_root.trim().is_empty() {
            self.project_plans.clear();
            self.selected_projects.clear();
            return;
        }
        self.project_plans = build_plans(&self.projects, &root);
        self.selected_projects = self
            .project_plans
            .iter()
            .enumerate()
            .filter_map(|(index, plan)| plan.safe_to_move.then_some(index))
            .collect();
    }

    fn draw_root_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("扫描范围：");
            if ui.button("选择文件夹…").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
            {
                self.add_root(path);
            }
            for drive in ["C:\\", "D:\\", "E:\\"] {
                if Path::new(drive).is_dir() && ui.small_button(format!("添加 {drive}")).clicked() {
                    self.add_root(PathBuf::from(drive));
                }
            }
            if ui.button("清空").clicked() && !self.running {
                self.roots.clear();
            }
        });
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.root_text);
            if ui.button("添加路径").clicked() {
                let path = PathBuf::from(self.root_text.trim());
                self.add_root(path);
                self.root_text.clear();
            }
        });
        let mut remove = None;
        egui::ScrollArea::horizontal().max_height(52.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (index, root) in self.roots.iter().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(root.display().to_string());
                            if ui.small_button("×").clicked() && !self.running {
                                remove = Some(index);
                            }
                        });
                    });
                }
            });
        });
        if let Some(index) = remove {
            self.roots.remove(index);
        }
    }

    fn draw_task_status(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.small(format!("阶段：{}", self.progress.stage));
            if self.running {
                ui.spinner();
                if ui.button("取消").clicked()
                    && let Some(cancel) = &self.cancel
                {
                    cancel.cancel();
                }
            }
            ui.label(&self.progress.message);
        });
        if self.progress.total > 0 {
            let fraction = (self.progress.current as f32 / self.progress.total as f32).clamp(0.0, 1.0);
            ui.add(egui::ProgressBar::new(fraction).show_percentage());
        }
    }

    fn draw_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::LargeFiles, "大文件");
            ui.selectable_value(&mut self.tab, Tab::Duplicates, "重复文件");
            ui.selectable_value(&mut self.tab, Tab::Projects, "工程整理");
            ui.selectable_value(&mut self.tab, Tab::Audit, "日志与安全");
        });
        ui.separator();
    }

    fn draw_large_files(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("最小大小（GB）：");
            ui.add(
                egui::DragValue::new(&mut self.large_threshold_gb)
                    .range(0.001..=100_000.0)
                    .speed(0.5),
            );
            if ui
                .add_enabled(
                    !self.running && !self.roots.is_empty(),
                    egui::Button::new("开始扫描"),
                )
                .clicked()
            {
                let roots = self.current_roots();
                let min_size = (self.large_threshold_gb * 1024.0 * 1024.0 * 1024.0) as u64;
                self.progress.message = "准备扫描大文件…".to_owned();
                self.start_task(move |tx, cancel, progress| {
                    match scan_large_files(&roots, min_size, &cancel, Some(progress)) {
                        Ok(files) => {
                            let _ = tx.send(WorkerEvent::LargeFiles(files));
                        }
                        Err(_error) if cancel.is_cancelled() => {
                            let _ = tx.send(WorkerEvent::Cancelled);
                        }
                        Err(error) => {
                            let _ = tx.send(WorkerEvent::Failed(error.to_string()));
                        }
                    }
                });
            }
            if ui.button("导出 CSV").clicked() && !self.large_files.is_empty() {
                self.export_large_files();
            }
            ui.label(format!("结果：{}", self.large_files.len()));
        });
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("large_files_grid")
                .striped(true)
                .min_col_width(120.0)
                .show(ui, |ui| {
                    ui.strong("大小");
                    ui.strong("占用");
                    ui.strong("修改时间");
                    ui.strong("路径");
                    ui.end_row();
                    for file in self.large_files.iter().take(self.max_visible_rows) {
                        ui.label(human_size(file.size_bytes));
                        ui.label(human_size(file.allocated_bytes));
                        ui.label(format_unix(file.modified_unix));
                        if ui.link(file.path.display().to_string()).clicked() {
                            let target = file.path.parent().unwrap_or(&file.path);
                            let _ = open::that(target);
                        }
                        ui.end_row();
                    }
                });
        });
    }

    fn export_large_files(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("large-files.csv")
            .save_file()
        {
            match export_serializable(&self.large_files, &path) {
                Ok(()) => self.add_log(format!("已导出：{}", path.display())),
                Err(error) => self.add_log(format!("导出失败：{error}")),
            }
        }
    }

    fn draw_duplicates(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("最小大小（MB）：");
            ui.add(egui::DragValue::new(&mut self.duplicate_threshold_mb).range(0.001..=1_000_000.0));
            if ui
                .add_enabled(
                    !self.running && !self.roots.is_empty(),
                    egui::Button::new("精确查重"),
                )
                .clicked()
            {
                let roots = self.current_roots();
                let min_size = (self.duplicate_threshold_mb * 1024.0 * 1024.0) as u64;
                self.start_task(move |tx, cancel, progress| {
                    match scan_duplicates(&roots, min_size, &cancel, Some(progress)) {
                        Ok(groups) => {
                            let _ = tx.send(WorkerEvent::Duplicates(groups));
                        }
                        Err(_error) if cancel.is_cancelled() => {
                            let _ = tx.send(WorkerEvent::Cancelled);
                        }
                        Err(error) => {
                            let _ = tx.send(WorkerEvent::Failed(error.to_string()));
                        }
                    }
                });
            }
            if ui.button("每组保留第一份").clicked() {
                self.selected_duplicates.clear();
                for group in &self.duplicate_groups {
                    self.selected_duplicates
                        .extend(group.paths.iter().skip(1).cloned());
                }
            }
            if ui.button("清除选择").clicked() {
                self.selected_duplicates.clear();
            }
            if ui
                .add_enabled(
                    !self.selected_duplicates.is_empty() && !self.running,
                    egui::Button::new("选中项移入回收站"),
                )
                .clicked()
            {
                self.confirm_recycle = true;
            }
            if ui.button("导出 CSV").clicked() && !self.duplicate_groups.is_empty() {
                self.export_duplicates();
            }
        });
        let reclaimable: u64 = self
            .duplicate_groups
            .iter()
            .map(DuplicateGroup::reclaimable_bytes)
            .sum();
        ui.label(format!(
            "重复组：{}，理论可释放：{}，已选择：{} 个文件",
            self.duplicate_groups.len(),
            human_size(reclaimable),
            self.selected_duplicates.len()
        ));
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let mut shown = 0;
            for group in &self.duplicate_groups {
                ui.collapsing(
                    format!(
                        "{} · {} · {} 份 · 可释放 {}",
                        group.id,
                        human_size(group.size_bytes),
                        group.paths.len(),
                        human_size(group.reclaimable_bytes())
                    ),
                    |ui| {
                        ui.small(format!("SHA-256: {}", group.sha256));
                        for path in &group.paths {
                            if shown >= self.max_visible_rows {
                                break;
                            }
                            let mut selected = self.selected_duplicates.contains(path);
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut selected, "").changed() {
                                    if selected {
                                        self.selected_duplicates.insert(path.clone());
                                    } else {
                                        self.selected_duplicates.remove(path);
                                    }
                                }
                                if ui.link(path.display().to_string()).clicked() {
                                    let _ = open::that(path.parent().unwrap_or(path));
                                }
                            });
                            shown += 1;
                        }
                    },
                );
            }
        });
    }

    fn export_duplicates(&mut self) {
        #[derive(serde::Serialize)]
        struct Row<'a> {
            group_id: &'a str,
            sha256: &'a str,
            size_bytes: u64,
            path: String,
        }
        let rows: Vec<_> = self
            .duplicate_groups
            .iter()
            .flat_map(|group| {
                group.paths.iter().map(move |path| Row {
                    group_id: &group.id,
                    sha256: &group.sha256,
                    size_bytes: group.size_bytes,
                    path: path.display().to_string(),
                })
            })
            .collect();
        if let Some(path) = rfd::FileDialog::new().set_file_name("duplicates.csv").save_file() {
            match export_serializable(&rows, &path) {
                Ok(()) => self.add_log(format!("已导出：{}", path.display())),
                Err(error) => self.add_log(format!("导出失败：{error}")),
            }
        }
    }

    fn draw_projects(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("工程库：");
            ui.text_edit_singleline(&mut self.library_root);
            if ui.button("选择…").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
            {
                self.library_root = path.display().to_string();
                self.rebuild_project_plans();
            }
            if ui
                .add_enabled(
                    !self.running && !self.roots.is_empty(),
                    egui::Button::new("识别工程"),
                )
                .clicked()
            {
                let roots = self.current_roots();
                self.start_task(move |tx, cancel, progress| {
                    match scan_projects(&roots, &cancel, Some(progress)) {
                        Ok(projects) => {
                            let _ = tx.send(WorkerEvent::Projects(projects));
                        }
                        Err(_error) if cancel.is_cancelled() => {
                            let _ = tx.send(WorkerEvent::Cancelled);
                        }
                        Err(error) => {
                            let _ = tx.send(WorkerEvent::Failed(error.to_string()));
                        }
                    }
                });
            }
            if ui.button("重建计划").clicked() {
                self.rebuild_project_plans();
            }
            if ui.button("选择全部安全项目").clicked() {
                self.selected_projects = self
                    .project_plans
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| p.safe_to_move.then_some(i))
                    .collect();
            }
            if ui.button("清除选择").clicked() {
                self.selected_projects.clear();
            }
            if ui
                .add_enabled(
                    !self.running && !self.selected_projects.is_empty(),
                    egui::Button::new("整理选中项目"),
                )
                .clicked()
            {
                self.confirm_move = true;
            }
            if ui.button("导出计划").clicked() && !self.project_plans.is_empty() {
                self.export_project_plans();
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.allow_cross_drive, "允许跨盘移动（默认关闭，可能较慢）");
            ui.label(format!(
                "发现：{}，计划：{}，已选择：{}",
                self.projects.len(),
                self.project_plans.len(),
                self.selected_projects.len()
            ));
        });

        if let Some(index) = self.edit_project_index
            && index < self.project_plans.len()
        {
            ui.group(|ui| {
                ui.label("编辑选中项目的名称和分类：");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.edit_project_name);
                    egui::ComboBox::from_id_salt("edit_category")
                        .selected_text(&self.edit_project_category)
                        .show_ui(ui, |ui| {
                            for category in categories() {
                                ui.selectable_value(
                                    &mut self.edit_project_category,
                                    (*category).to_owned(),
                                    *category,
                                );
                            }
                        });
                    if ui.button("应用到计划").clicked() {
                        self.apply_project_edit(index);
                    }
                });
            });
        }

        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("project_grid")
                .striped(true)
                .min_col_width(90.0)
                .show(ui, |ui| {
                    ui.strong("选");
                    ui.strong("新名称");
                    ui.strong("分类");
                    ui.strong("语言");
                    ui.strong("Git");
                    ui.strong("安全");
                    ui.strong("源路径");
                    ui.end_row();
                    for index in 0..self.project_plans.len().min(self.max_visible_rows) {
                        let plan = &self.project_plans[index];
                        let mut selected = self.selected_projects.contains(&index);
                        if ui
                            .add_enabled(plan.safe_to_move, egui::Checkbox::new(&mut selected, ""))
                            .changed()
                        {
                            if selected {
                                self.selected_projects.insert(index);
                            } else {
                                self.selected_projects.remove(&index);
                            }
                        }
                        if ui
                            .selectable_label(self.edit_project_index == Some(index), &plan.name)
                            .clicked()
                        {
                            self.edit_project_index = Some(index);
                            self.edit_project_name = plan.name.clone();
                            self.edit_project_category = plan.category.clone();
                        }
                        ui.label(&plan.category);
                        ui.label(plan.languages.join(" + "));
                        ui.label(if plan.expected_head.is_empty() {
                            "无/未初始化"
                        } else {
                            "已记录"
                        });
                        ui.label(if plan.safe_to_move {
                            "安全"
                        } else {
                            &plan.safety_reason
                        });
                        if ui.link(plan.source.display().to_string()).clicked() {
                            let _ = open::that(&plan.source);
                        }
                        ui.end_row();
                    }
                });
        });
    }

    fn apply_project_edit(&mut self, index: usize) {
        let name = self.edit_project_name.trim();
        if name.is_empty() || !categories().contains(&self.edit_project_category.as_str()) {
            self.add_log("名称或分类无效");
            return;
        }
        let destination = {
            let Some(plan) = self.project_plans.get_mut(index) else {
                return;
            };
            let Some(library) = plan
                .destination
                .ancestors()
                .find(|path| path.ends_with(&plan.category))
                .and_then(Path::parent)
                .map(Path::to_path_buf)
            else {
                self.add_log("无法推导工程库根目录，请重建计划");
                return;
            };
            plan.name = crate::util::sanitize_name(name, "项目");
            plan.category.clone_from(&self.edit_project_category);
            plan.destination = library.join(&plan.category).join(&plan.name);
            plan.destination.clone()
        };
        self.add_log(format!("计划已更新：{}", destination.display()));
    }

    fn export_project_plans(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("project-plan.csv")
            .save_file()
        {
            match export_serializable(&self.project_plans, &path) {
                Ok(()) => self.add_log(format!("已导出：{}", path.display())),
                Err(error) => self.add_log(format!("导出失败：{error}")),
            }
        }
    }

    fn draw_audit(&mut self, ui: &mut egui::Ui) {
        ui.heading("安全边界");
        ui.label("• 扫描只读；重复文件仅移入回收站；每组至少保留一份。");
        ui.label("• 工程按根目录整体移动，不把两个仓库逐文件合并。");
        ui.label("• Git worktree、重分析点、已有目标和跨盘移动默认拒绝。");
        ui.label("• 变更后核对 Git HEAD、分支和远程地址；失败自动回滚同盘移动。");
        ui.horizontal(|ui| {
            ui.label(format!("审计日志：{}", audit_log_path().display()));
            if ui.button("打开日志目录").clicked() {
                let target = audit_log_path()
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                let _ = open::that(target);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for line in &self.logs {
                ui.monospace(line);
            }
        });
    }

    fn draw_confirmations(&mut self, context: &egui::Context) {
        if self.confirm_recycle {
            egui::Window::new("确认移入回收站")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!(
                        "将 {} 个选中文件移入 Windows 回收站。",
                        self.selected_duplicates.len()
                    ));
                    ui.label("工具会阻止删除某一重复组的全部副本。");
                    ui.horizontal(|ui| {
                        if ui.button("确认").clicked() {
                            self.confirm_recycle = false;
                            let groups = self.duplicate_groups.clone();
                            let selected = self.selected_duplicates.clone();
                            self.start_task(move |tx, _cancel, _progress| {
                                let record = recycle_duplicates(&groups, &selected);
                                let _ = tx.send(WorkerEvent::RecycleResult(record));
                            });
                        }
                        if ui.button("取消").clicked() {
                            self.confirm_recycle = false;
                        }
                    });
                });
        }
        if self.confirm_move {
            egui::Window::new("确认整理工程")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!(
                        "将整体移动 {} 个安全工程。",
                        self.selected_projects.len()
                    ));
                    ui.label(".git 和项目全部文件会一起移动；不会合并两个仓库的内容。");
                    ui.horizontal(|ui| {
                        if ui.button("确认执行").clicked() {
                            self.confirm_move = false;
                            let plans = self.project_plans.clone();
                            let selected = self.selected_projects.clone();
                            let allow_cross_drive = self.allow_cross_drive;
                            self.start_task(move |tx, _cancel, _progress| {
                                let records = move_projects(&plans, &selected, allow_cross_drive);
                                let _ = tx.send(WorkerEvent::ProjectOperations(records));
                            });
                        }
                        if ui.button("取消").clicked() {
                            self.confirm_move = false;
                        }
                    });
                });
        }
    }
}

impl eframe::App for OrganizerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_worker_events();
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, egui::Color32::from_rgb(247, 245, 239));
        ui.heading("磁盘工程整理助手");
        self.draw_root_selector(ui);
        self.draw_tabs(ui);
        match self.tab {
            Tab::LargeFiles => self.draw_large_files(ui),
            Tab::Duplicates => self.draw_duplicates(ui),
            Tab::Projects => self.draw_projects(ui),
            Tab::Audit => self.draw_audit(ui),
        }
        self.draw_task_status(ui);
        self.draw_confirmations(ui.ctx());
        if self.running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn install_chinese_font(context: &egui::Context) {
    for candidate in [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
    ] {
        let Ok(bytes) = std::fs::read(candidate) else {
            continue;
        };
        let mut definitions = egui::FontDefinitions::default();
        definitions
            .font_data
            .insert("windows-cjk".to_owned(), egui::FontData::from_owned(bytes).into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            definitions
                .families
                .entry(family)
                .or_default()
                .insert(0, "windows-cjk".to_owned());
        }
        context.set_fonts(definitions);
        break;
    }
}
