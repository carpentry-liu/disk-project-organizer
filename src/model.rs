use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LargeFile {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub allocated_bytes: u64,
    pub modified_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub id: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub paths: Vec<PathBuf>,
}

impl DuplicateGroup {
    #[must_use]
    pub fn reclaimable_bytes(&self) -> u64 {
        self.size_bytes
            .saturating_mul(self.paths.len().saturating_sub(1) as u64)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GitInfo {
    pub marker_kind: String,
    pub head: String,
    pub branch: String,
    pub remote: String,
    pub worktree_count: usize,
    pub tracked_dirty: bool,
    pub valid: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub path: PathBuf,
    pub original_name: String,
    pub suggested_name: String,
    pub description: String,
    pub languages: Vec<String>,
    pub category: String,
    pub markers: Vec<String>,
    pub git: GitInfo,
    pub safe_to_move: bool,
    pub safety_reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectPlan {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub name: String,
    pub category: String,
    pub description: String,
    pub languages: Vec<String>,
    pub safe_to_move: bool,
    pub safety_reason: String,
    pub expected_head: String,
    pub expected_branch: String,
    pub expected_remote: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationRecord {
    pub success: bool,
    pub action: String,
    pub source: String,
    pub destination: String,
    pub message: String,
    pub time: String,
}

#[derive(Clone, Debug)]
pub struct ProgressUpdate {
    pub stage: String,
    pub message: String,
    pub current: u64,
    pub total: u64,
}

#[derive(Clone, Debug)]
pub enum WorkerEvent {
    Progress(ProgressUpdate),
    LargeFiles(Vec<LargeFile>),
    Duplicates(Vec<DuplicateGroup>),
    Projects(Vec<ProjectInfo>),
    ProjectOperations(Vec<OperationRecord>),
    RecycleResult(OperationRecord),
    Failed(String),
    Cancelled,
}
