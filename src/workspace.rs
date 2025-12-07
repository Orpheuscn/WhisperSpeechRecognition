use std::path::{Path, PathBuf};
use std::fs;
use serde::{Serialize, Deserialize};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub video_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
    pub cut_points: Vec<f64>,
    pub audio_segments: Vec<PathBuf>,
    #[serde(default)]  // 兼容旧的 workspace_state.json，如果没有这个字段就用空数组
    pub completed_segments: Vec<usize>,  // 已完成识别的片段索引
    pub manual_segment: Option<PathBuf>,
    #[serde(default)]  // 兼容旧版本
    pub manual_start_time: String,
    #[serde(default)]  // 兼容旧版本
    pub manual_end_time: String,
    pub total_duration: f64,
    pub workspace_dir: PathBuf,
}

impl WorkspaceState {
    pub fn save(&self, workspace_dir: &Path) -> Result<()> {
        let state_file = workspace_dir.join("workspace_state.json");
        let json = serde_json::to_string_pretty(self)?;
        fs::write(state_file, json)?;
        Ok(())
    }
    
    pub fn load(workspace_dir: &Path) -> Result<Self> {
        let state_file = workspace_dir.join("workspace_state.json");
        let json = fs::read_to_string(state_file)?;
        let mut state: WorkspaceState = serde_json::from_str(&json)?;
        state.workspace_dir = workspace_dir.to_path_buf();
        Ok(state)
    }
    
    pub fn exists(workspace_dir: &Path) -> bool {
        workspace_dir.join("workspace_state.json").exists()
    }
}

/// 创建工作区目录结构
pub fn create_workspace_structure(base_dir: &Path) -> Result<()> {
    fs::create_dir_all(base_dir)?;
    fs::create_dir_all(base_dir.join("segments"))?;
    fs::create_dir_all(base_dir.join("subtitles"))?;
    Ok(())
}

/// 检查路径是否在工作区内
#[allow(dead_code)]
pub fn is_in_workspace(path: &Path, workspace_dir: &Path) -> bool {
    path.starts_with(workspace_dir)
}

