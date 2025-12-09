use std::path::PathBuf;
use std::sync::mpsc::Receiver;

/// 应用状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppState {
    Idle,
    AudioExtracted,
    Processing,
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Idle
    }
}

/// 进度消息
#[derive(Debug, Clone)]
pub enum ProgressMessage {
    Progress { current: usize, total: usize },
    Result { segment: usize, text: String },
    RealtimeOutput(String),
    Completed,
    Error(String),
}

/// Whisper模型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WhisperModel {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
    Turbo,
}

impl Default for WhisperModel {
    fn default() -> Self {
        WhisperModel::Base
    }
}

impl WhisperModel {
    pub fn as_str(&self) -> &str {
        match self {
            WhisperModel::Tiny => "tiny",
            WhisperModel::Base => "base",
            WhisperModel::Small => "small",
            WhisperModel::Medium => "medium",
            WhisperModel::Large => "large",
            WhisperModel::Turbo => "turbo",
        }
    }
    
    pub fn all() -> Vec<WhisperModel> {
        vec![
            WhisperModel::Tiny,
            WhisperModel::Base,
            WhisperModel::Small,
            WhisperModel::Medium,
            WhisperModel::Large,
            WhisperModel::Turbo,
        ]
    }
}

/// Whisper语言
#[derive(Debug, Clone, PartialEq)]
pub enum WhisperLanguage {
    Unknown,
    Japanese,
    English,
    Chinese,
    French,
    German,
    Spanish,
    Italian,
    Russian,
    Custom,
}

impl Default for WhisperLanguage {
    fn default() -> Self {
        WhisperLanguage::Unknown
    }
}

impl WhisperLanguage {
    pub fn as_str(&self) -> &str {
        match self {
            WhisperLanguage::Unknown => "Auto Detect",
            WhisperLanguage::Japanese => "Japanese",
            WhisperLanguage::English => "English",
            WhisperLanguage::Chinese => "Chinese",
            WhisperLanguage::French => "French",
            WhisperLanguage::German => "German",
            WhisperLanguage::Spanish => "Spanish",
            WhisperLanguage::Italian => "Italian",
            WhisperLanguage::Russian => "Russian",
            WhisperLanguage::Custom => "Custom (Manual Input)",
        }
    }
    
    pub fn all() -> Vec<WhisperLanguage> {
        vec![
            WhisperLanguage::Unknown,
            WhisperLanguage::English,
            WhisperLanguage::Chinese,
            WhisperLanguage::Japanese,
            WhisperLanguage::French,
            WhisperLanguage::German,
            WhisperLanguage::Spanish,
            WhisperLanguage::Italian,
            WhisperLanguage::Russian,
            WhisperLanguage::Custom,
        ]
    }
    
    pub fn to_code<'a>(&self, custom_code: &'a str) -> Option<&'a str> {
        match self {
            WhisperLanguage::Unknown => None,
            WhisperLanguage::Japanese => Some("ja"),
            WhisperLanguage::English => Some("en"),
            WhisperLanguage::Chinese => Some("zh"),
            WhisperLanguage::French => Some("fr"),
            WhisperLanguage::German => Some("de"),
            WhisperLanguage::Spanish => Some("es"),
            WhisperLanguage::Italian => Some("it"),
            WhisperLanguage::Russian => Some("ru"),
            WhisperLanguage::Custom => {
                if custom_code.is_empty() {
                    None
                } else {
                    Some(custom_code)
                }
            }
        }
    }
}

/// 识别模式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecognitionMode {
    Normal,    // 普通模式（切割后识别）
    VAD,       // VAD模式（连续语音检测）
}

impl Default for RecognitionMode {
    fn default() -> Self {
        RecognitionMode::Normal
    }
}

/// 主应用结构体
#[derive(Default)]
pub struct WhisperApp {
    // 文件路径
    pub video_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
    pub srt_path: Option<PathBuf>,  // 加载的字幕文件路径
    
    // 应用状态
    pub state: AppState,
    pub status_message: String,
    
    // 视频/音频播放器
    pub video_player: Option<crate::video_player::VideoPlayer>,
    pub is_playing: bool,
    pub current_position: f64, // 秒
    pub total_duration: f64,   // 秒
    
    // 切割点
    pub cut_points: Vec<f64>,  // 时间点（秒）
    
    // Whisper 参数
    pub whisper_model: WhisperModel,
    pub whisper_language: WhisperLanguage,
    pub custom_language_code: String,
    
    // 识别模式
    pub recognition_mode: RecognitionMode,
    
    // 切割后的音频文件
    pub audio_segments: Vec<PathBuf>,
    
    // 进度信息
    pub processing_progress: f32,
    pub processing_status: String,
    
    // 识别结果
    pub recognition_results: Vec<String>,
    
    // 消息通道
    pub progress_receiver: Option<Receiver<ProgressMessage>>,
    
    // 手动切割
    pub manual_start_time: String,
    pub manual_end_time: String,
    pub manual_segment: Option<PathBuf>,
    
    // 工作区
    pub workspace_dir: Option<PathBuf>,
    pub can_resume: bool,
    pub missing_segments: Vec<usize>,
    pub completed_segments: Vec<usize>,
    
    // 字幕相关（新增）
    pub subtitles: Vec<crate::subtitle::SubtitleEntry>,
    pub selected_subtitle_index: Option<usize>,
    pub subtitle_scroll_offset: f32,
}

impl WhisperApp {
    pub fn format_time(seconds: f64) -> String {
        let hours = (seconds / 3600.0).floor() as u32;
        let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
        let secs = (seconds % 60.0).floor() as u32;
        let millis = ((seconds % 1.0) * 1000.0).floor() as u32;
        
        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, secs, millis)
    }
}

