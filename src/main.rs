mod audio_player;
mod ffmpeg;
mod whisper;
mod srt_merger;
mod recognition;
mod manual_cut;
mod workspace;

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::fs;
use std::process::Command;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 700.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Whisper Speech Recognition",
        options,
        Box::new(|_cc| Ok(Box::new(WhisperApp::default()))),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AppState {
    Idle,
    AudioExtracted,
    Processing,
}

#[derive(Default)]
struct WhisperApp {
    // 文件路径
    video_path: Option<PathBuf>,
    audio_path: Option<PathBuf>,
    
    // 应用状态
    state: AppState,
    status_message: String,
    
    // 音频播放器
    audio_player: Option<audio_player::AudioPlayer>,
    is_playing: bool,
    current_position: f64, // 秒
    total_duration: f64,   // 秒
    
    // 切割点
    cut_points: Vec<f64>,  // 时间点（秒）
    
    // Whisper 参数
    whisper_model: WhisperModel,
    whisper_language: WhisperLanguage,
    custom_language_code: String,
    
    // 切割后的音频文件
    audio_segments: Vec<PathBuf>,
    
    // 进度信息
    processing_progress: f32,
    processing_status: String,
    
    // 识别结果
    recognition_results: Vec<String>,
    
    // 消息通道
    progress_receiver: Option<Receiver<ProgressMessage>>,
    
    // 重新识别
    selected_segment_index: usize,
    
    // 手动切割
    manual_start_time: String,
    manual_end_time: String,
    manual_segment: Option<PathBuf>,
    
    // 工作区
    workspace_dir: Option<PathBuf>,
    can_resume: bool,  // 是否可以恢复识别
    missing_segments: Vec<usize>,  // 缺失字幕的片段索引
    completed_segments: Vec<usize>,  // 已完成的片段索引
}

#[derive(Debug, Clone)]
enum ProgressMessage {
    Progress { current: usize, total: usize },
    Result { segment: usize, text: String },
    RealtimeOutput(String),  // 实时输出信息
    Completed,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WhisperModel {
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
    fn as_str(&self) -> &str {
        match self {
            WhisperModel::Tiny => "tiny",
            WhisperModel::Base => "base",
            WhisperModel::Small => "small",
            WhisperModel::Medium => "medium",
            WhisperModel::Large => "large",
            WhisperModel::Turbo => "turbo",
        }
    }
    
    fn all() -> Vec<WhisperModel> {
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

#[derive(Debug, Clone, PartialEq)]
enum WhisperLanguage {
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
    fn as_str(&self) -> &str {
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
    
    fn all() -> Vec<WhisperLanguage> {
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
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Idle
    }
}

impl WhisperApp {
    fn handle_dropped_file(&mut self, path: PathBuf) {
        self.video_path = Some(path.clone());
        self.state = AppState::Idle;
        self.status_message = format!("File loaded: {:?}", path.file_name().unwrap());
        self.audio_path = None;
        self.audio_player = None;
        self.cut_points.clear();
        self.audio_segments.clear();
        self.recognition_results.clear();
        
        // 重置工作区（新视频需要新工作区）
        self.workspace_dir = None;
        
        // 检查文件类型：如果是音频文件，直接使用；如果是视频，提取音频
        let extension = path.extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        
        if matches!(extension.as_str(), "wav" | "mp3" | "m4a" | "flac" | "ogg" | "opus") {
            // 直接使用音频文件
            self.load_audio_file(path);
        } else {
            // 从视频中提取音频
            self.extract_audio();
        }
    }
    
    fn load_audio_file(&mut self, audio_path: PathBuf) {
        self.audio_path = Some(audio_path.clone());
        self.status_message = "Audio file loaded!".to_string();
        self.state = AppState::AudioExtracted;
        
        // 加载音频播放器
        match audio_player::AudioPlayer::new(&audio_path) {
            Ok(player) => {
                self.total_duration = player.duration();
                self.audio_player = Some(player);
            }
            Err(e) => {
                self.status_message = format!("Failed to load audio: {}", e);
            }
        }
    }
    
    fn extract_audio(&mut self) {
        if let Some(video_path) = &self.video_path {
            self.status_message = "Extracting audio...".to_string();
            
            match ffmpeg::extract_audio(video_path) {
                Ok(audio_path) => {
                    self.audio_path = Some(audio_path.clone());
                    self.status_message = "Audio extracted successfully!".to_string();
                    self.state = AppState::AudioExtracted;
                    
                    // Load audio player
                    match audio_player::AudioPlayer::new(&audio_path) {
                        Ok(player) => {
                            self.total_duration = player.duration();
                            self.audio_player = Some(player);
                        }
                        Err(e) => {
                            self.status_message = format!("Failed to load audio: {}", e);
                        }
                    }
                }
                Err(e) => {
                    self.status_message = format!("Failed to extract audio: {}", e);
                }
            }
        }
    }
    
    fn add_cut_point(&mut self) {
        if !self.cut_points.contains(&self.current_position) {
            self.cut_points.push(self.current_position);
            self.cut_points.sort_by(|a, b| a.partial_cmp(b).unwrap());
        }
    }
    
    fn remove_cut_point(&mut self, index: usize) {
        if index < self.cut_points.len() {
            self.cut_points.remove(index);
        }
    }
    
    fn cut_audio(&mut self) {
        if let Some(audio_path) = &self.audio_path {
            self.status_message = "Cutting audio...".to_string();
            self.state = AppState::Processing;
            
            match ffmpeg::cut_audio(audio_path, &self.cut_points) {
                Ok(segments) => {
                    self.audio_segments = segments;
                    self.status_message = format!("Audio cut completed, {} segments", self.audio_segments.len());
                    self.state = AppState::AudioExtracted;
                }
                Err(e) => {
                    self.status_message = format!("Failed to cut audio: {}", e);
                    self.state = AppState::AudioExtracted;
                }
            }
        }
    }
    
    fn start_recognition(&mut self) {
        if self.audio_segments.is_empty() {
            self.status_message = "Please cut audio first!".to_string();
            return;
        }
        
        self.state = AppState::Processing;
        self.processing_progress = 0.0;
        self.processing_status = "Starting recognition...".to_string();
        self.recognition_results.clear();
        
        let segments = self.audio_segments.clone();
        let model = self.whisper_model;
        let language = self.whisper_language.clone();
        let custom_lang = self.custom_language_code.clone();
        let cut_points = self.cut_points.clone();
        let video_path = self.video_path.clone().unwrap();
        
        // 创建消息通道
        let (tx, rx) = channel();
        self.progress_receiver = Some(rx);
        
        std::thread::spawn(move || {
            let total = segments.len();
            let mut srt_files = Vec::new();
            
            for (i, segment) in segments.iter().enumerate() {
                // 确定要使用的语言代码
                let lang_code = match language {
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
                        if custom_lang.is_empty() {
                            None
                        } else {
                            Some(custom_lang.as_str())
                        }
                    }
                };
                
                // 使用新的实时输出版本
                match whisper::recognize_audio_realtime(segment, model, lang_code, tx.clone(), i + 1, total) {
                    Ok((srt_path, text)) => {
                        srt_files.push(srt_path);
                        // 发送识别结果
                        let _ = tx.send(ProgressMessage::Result { 
                            segment: i + 1, 
                            text 
                        });
                        // 发送进度（识别完成后）
                        let _ = tx.send(ProgressMessage::Progress { 
                            current: i + 1, 
                            total 
                        });
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to recognize segment {}: {}", i + 1, e);
                        eprintln!("{}", error_msg);
                        let _ = tx.send(ProgressMessage::Error(error_msg));
                    }
                }
            }
            
            // Merge subtitles
            if !srt_files.is_empty() {
                let output_path = video_path.with_extension("srt");
                match srt_merger::merge_srt_files(&srt_files, &cut_points, &output_path) {
                    Ok(_) => {
                        println!("Subtitles merged successfully: {:?}", output_path);
                    }
                    Err(e) => {
                        eprintln!("Failed to merge subtitles: {}", e);
                        let _ = tx.send(ProgressMessage::Error(format!("Failed to merge: {}", e)));
                    }
                }
            }
            
            // 发送完成消息
            let _ = tx.send(ProgressMessage::Completed);
        });
    }
    
    fn format_time(seconds: f64) -> String {
        let hours = (seconds / 3600.0).floor() as u32;
        let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
        let secs = (seconds % 60.0).floor() as u32;
        let millis = ((seconds % 1.0) * 1000.0).floor() as u32;
        
        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, secs, millis)
    }
    
    fn cleanup_temp_files(&mut self) {
        // 删除提取的音频文件
        if let Some(audio_path) = &self.audio_path {
            if audio_path.exists() {
                let _ = fs::remove_file(audio_path);
            }
        }
        
        // 删除切割的音频片段和对应的字幕文件
        for segment in &self.audio_segments {
            if segment.exists() {
                let _ = fs::remove_file(segment);
            }
            
            // 删除对应的 SRT 文件
            let srt_path = segment.with_extension("srt");
            if srt_path.exists() {
                let _ = fs::remove_file(&srt_path);
            }
        }
        
        // 删除手动切割的片段
        if let Some(manual_seg) = &self.manual_segment {
            if manual_seg.exists() {
                let _ = fs::remove_file(manual_seg);
            }
            let srt_path = manual_seg.with_extension("srt");
            if srt_path.exists() {
                let _ = fs::remove_file(&srt_path);
            }
        }
        
        self.status_message = "Temporary files cleaned up.".to_string();
    }
    
    fn rerecognize_segment(&mut self) {
        if self.audio_segments.is_empty() || self.selected_segment_index >= self.audio_segments.len() {
            self.status_message = "Invalid segment selection!".to_string();
            return;
        }
        
        self.state = AppState::Processing;
        self.processing_progress = 0.0;
        self.processing_status = "Re-recognizing segment...".to_string();
        self.recognition_results.clear();
        
        let segment = self.audio_segments[self.selected_segment_index].clone();
        let segment_index = self.selected_segment_index;
        let all_segments = self.audio_segments.clone();
        let model = self.whisper_model;
        let language = self.whisper_language.clone();
        let custom_lang = self.custom_language_code.clone();
        let cut_points = self.cut_points.clone();
        let video_path = self.video_path.clone().unwrap();
        
        // 创建消息通道
        let (tx, rx) = channel();
        self.progress_receiver = Some(rx);
        
        std::thread::spawn(move || {
            // 重新识别单个片段
            match recognition::recognize_single_segment(
                &segment,
                segment_index,
                all_segments.len(),
                model,
                &language,
                &custom_lang,
                tx.clone(),
            ) {
                Ok((_srt_path, text)) => {
                    let _ = tx.send(ProgressMessage::Result { 
                        segment: segment_index + 1, 
                        text 
                    });
                    let _ = tx.send(ProgressMessage::Progress { 
                        current: 1, 
                        total: 1 
                    });
                    
                    // 收集所有字幕文件并重新合并
                    let mut srt_files = Vec::new();
                    for seg in &all_segments {
                        let srt = seg.with_extension("srt");
                        if srt.exists() {
                            srt_files.push(srt);
                        }
                    }
                    
                    // 重新合并字幕
                    if !srt_files.is_empty() {
                        let output_path = video_path.with_extension("srt");
                        match recognition::remerge_subtitles(&srt_files, &cut_points, &output_path) {
                            Ok(_) => {
                                println!("Subtitles remerged successfully: {:?}", output_path);
                            }
                            Err(e) => {
                                eprintln!("Failed to remerge subtitles: {}", e);
                                let _ = tx.send(ProgressMessage::Error(format!("Failed to remerge: {}", e)));
                            }
                        }
                    }
                }
                Err(e) => {
                    let error_msg = format!("Failed to re-recognize segment {}: {}", segment_index + 1, e);
                    eprintln!("{}", error_msg);
                    let _ = tx.send(ProgressMessage::Error(error_msg));
                }
            }
            
            let _ = tx.send(ProgressMessage::Completed);
        });
    }
    
    fn cut_manual_segment(&mut self) {
        if let Some(audio_path) = &self.audio_path {
            // 解析时间
            let start_time = match manual_cut::parse_time_string(&self.manual_start_time) {
                Ok(t) => t,
                Err(_) => {
                    self.status_message = "Invalid start time format!".to_string();
                    return;
                }
            };
            
            let end_time = match manual_cut::parse_time_string(&self.manual_end_time) {
                Ok(t) => t,
                Err(_) => {
                    self.status_message = "Invalid end time format!".to_string();
                    return;
                }
            };
            
            // 切割片段
            match manual_cut::cut_audio_segment(audio_path, start_time, end_time) {
                Ok(segment_path) => {
                    self.manual_segment = Some(segment_path);
                    self.status_message = format!("Manual segment cut: {:.2}s - {:.2}s", start_time, end_time);
                }
                Err(e) => {
                    self.status_message = format!("Failed to cut segment: {}", e);
                }
            }
        }
    }
    
    fn recognize_manual_segment(&mut self) {
        if self.manual_segment.is_none() {
            self.status_message = "No manual segment to recognize!".to_string();
            return;
        }
        
        self.state = AppState::Processing;
        self.processing_progress = 0.0;
        self.processing_status = "Recognizing manual segment...".to_string();
        self.recognition_results.clear();
        
        let segment = self.manual_segment.clone().unwrap();
        let model = self.whisper_model;
        let language = self.whisper_language.clone();
        let custom_lang = self.custom_language_code.clone();
        let video_path = self.video_path.clone().unwrap();
        let all_segments = self.audio_segments.clone();
        let cut_points = self.cut_points.clone();
        
        // 解析手动片段的起始时间
        let start_time = match manual_cut::parse_time_string(&self.manual_start_time) {
            Ok(t) => t,
            Err(_) => 0.0,
        };
        
        // 创建消息通道
        let (tx, rx) = channel();
        self.progress_receiver = Some(rx);
        
        std::thread::spawn(move || {
            // 识别手动片段
            match recognition::recognize_single_segment(
                &segment,
                0,
                1,
                model,
                &language,
                &custom_lang,
                tx.clone(),
            ) {
                Ok((_srt_path, text)) => {
                    let _ = tx.send(ProgressMessage::Result { 
                        segment: 0, 
                        text 
                    });
                    let _ = tx.send(ProgressMessage::Progress { 
                        current: 1, 
                        total: 1 
                    });
                    
                    // 收集所有字幕文件（包括手动片段）
                    let mut srt_files = Vec::new();
                    let mut segment_times = Vec::new();
                    
                    // 添加所有自动切割的片段
                    for (i, seg) in all_segments.iter().enumerate() {
                        let srt = seg.with_extension("srt");
                        if srt.exists() {
                            srt_files.push(srt);
                            // 计算每段的起始时间
                            if i == 0 {
                                segment_times.push((0.0, srt_files.len() - 1));
                            } else if i - 1 < cut_points.len() {
                                segment_times.push((cut_points[i - 1], srt_files.len() - 1));
                            }
                        }
                    }
                    
                    // 添加手动片段
                    let manual_srt = segment.with_extension("srt");
                    if manual_srt.exists() {
                        srt_files.push(manual_srt);
                        segment_times.push((start_time, srt_files.len() - 1));
                    }
                    
                    // 按时间排序
                    segment_times.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    
                    // 重新排列 srt_files
                    let mut sorted_srt_files = Vec::new();
                    let mut sorted_cut_points = Vec::new();
                    
                    for (time, idx) in segment_times {
                        sorted_srt_files.push(srt_files[idx].clone());
                        if time > 0.0 {
                            sorted_cut_points.push(time);
                        }
                    }
                    
                    // 合并字幕
                    if !sorted_srt_files.is_empty() {
                        let output_path = video_path.with_extension("srt");
                        match recognition::remerge_subtitles(&sorted_srt_files, &sorted_cut_points, &output_path) {
                            Ok(_) => {
                                println!("Subtitles merged successfully: {:?}", output_path);
                            }
                            Err(e) => {
                                eprintln!("Failed to merge subtitles: {}", e);
                                let _ = tx.send(ProgressMessage::Error(format!("Failed to merge: {}", e)));
                            }
                        }
                    }
                }
                Err(e) => {
                    let error_msg = format!("Failed to recognize manual segment: {}", e);
                    eprintln!("{}", error_msg);
                    let _ = tx.send(ProgressMessage::Error(error_msg));
                }
            }
            
            let _ = tx.send(ProgressMessage::Completed);
        });
    }
    
    fn stop_recognition(&mut self) {
        // 终止所有 whisper 和 python 进程
        Self::kill_whisper_processes();
        
        // 重置状态
        self.state = AppState::AudioExtracted;
        self.status_message = "Recognition stopped and all processes killed.".to_string();
        self.progress_receiver = None;
        self.processing_progress = 0.0;
        self.processing_status = String::new();
    }
    
    fn kill_whisper_processes() {
        // 查找并终止所有 whisper 相关进程
        if let Ok(output) = Command::new("ps")
            .args(&["aux"])
            .output()
        {
            let output_str = String::from_utf8_lossy(&output.stdout);
            
            for line in output_str.lines() {
                // 查找包含 whisper 的进程
                if line.contains("whisper") && !line.contains("grep") {
                    if let Some(pid) = Self::extract_pid_from_ps_line(line) {
                        let _ = Command::new("kill")
                            .args(&["-9", &pid.to_string()])
                            .output();
                    }
                }
                
                // 查找包含 python 且包含 whisper 的进程
                if line.contains("python") && line.contains("whisper") && !line.contains("grep") {
                    if let Some(pid) = Self::extract_pid_from_ps_line(line) {
                        let _ = Command::new("kill")
                            .args(&["-9", &pid.to_string()])
                            .output();
                    }
                }
            }
        }
    }
    
    fn extract_pid_from_ps_line(line: &str) -> Option<u32> {
        // ps aux 输出格式：USER PID ...
        // 提取第二列（PID）
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            parts[1].parse::<u32>().ok()
        } else {
            None
        }
    }
    
    fn save_plain_text(&self) {
        // 过滤出 Segment 开头的结果
        let mut plain_text = String::new();
        for result in &self.recognition_results {
            if result.starts_with("Segment") {
                plain_text.push_str(result);
                plain_text.push_str("\n\n");
            }
        }
        
        if plain_text.is_empty() {
            return;
        }
        
        // 生成默认文件名
        let video_name = self.video_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("transcript")
            .to_string();
        
        // 使用文件对话框保存
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&format!("{}_transcript.txt", video_name))
            .add_filter("Text", &["txt"])
            .save_file()
        {
            let _ = fs::write(path, plain_text);
        }
    }
    
    fn open_workspace(&mut self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            println!("Selected folder: {:?}", folder);
            
            // 检查是否存在工作区状态文件
            if workspace::WorkspaceState::exists(&folder) {
                println!("Found workspace_state.json, loading...");
                
                // 加载工作区
                match workspace::WorkspaceState::load(&folder) {
                    Ok(state) => {
                        println!("Workspace loaded successfully!");
                        println!("Video path: {:?}", state.video_path);
                        println!("Audio segments: {}", state.audio_segments.len());
                        println!("Completed segments: {:?}", state.completed_segments);
                        
                        self.workspace_dir = Some(folder.clone());
                        self.video_path = state.video_path.clone();
                        self.audio_path = state.audio_path.clone();
                        self.cut_points = state.cut_points.clone();
                        self.audio_segments = state.audio_segments.clone();
                        self.manual_segment = state.manual_segment.clone();
                        self.manual_start_time = state.manual_start_time.clone();
                        self.manual_end_time = state.manual_end_time.clone();
                        self.total_duration = state.total_duration;
                        
                        // 重新加载音频播放器
                        if let Some(audio_path) = &state.audio_path {
                            println!("Loading audio player from: {:?}", audio_path);
                            if audio_path.exists() {
                                match audio_player::AudioPlayer::new(audio_path) {
                                    Ok(player) => {
                                        println!("Audio player loaded successfully!");
                                        self.audio_player = Some(player);
                                        self.state = AppState::AudioExtracted;
                                    }
                                    Err(e) => {
                                        println!("Failed to load audio player: {}", e);
                                        self.state = AppState::AudioExtracted;  // 仍然设置状态
                                    }
                                }
                            } else {
                                println!("Audio file not found: {:?}", audio_path);
                                self.state = AppState::AudioExtracted;  // 即使音频不存在也设置状态
                            }
                        }
                        
                        // 检测哪些片段缺失字幕
                        self.check_missing_subtitles();
                        
                        let completed = self.completed_segments.len();
                        let total = self.audio_segments.len();
                        self.status_message = format!("Workspace loaded! {}/{} segments completed.", completed, total);
                    }
                    Err(e) => {
                        self.status_message = format!("Failed to load workspace: {}", e);
                        eprintln!("Error loading workspace: {}", e);
                    }
                }
            } else {
                println!("No workspace_state.json found, creating new workspace");
                
                // 创建新工作区
                match workspace::create_workspace_structure(&folder) {
                    Ok(_) => {
                        self.workspace_dir = Some(folder.clone());
                        self.status_message = format!("New workspace created: {:?}", folder);
                    }
                    Err(e) => {
                        self.status_message = format!("Failed to create workspace: {}", e);
                    }
                }
            }
        }
    }
    
    fn save_workspace(&mut self) {
        // 推荐工作区路径（基于视频文件所在目录）
        let default_dir = self.video_path.as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        
        // 每次保存都让用户选择或确认工作区位置
        let mut dialog = rfd::FileDialog::new();
        if let Some(dir) = default_dir {
            dialog = dialog.set_directory(dir);
        }
        
        if let Some(folder) = dialog.pick_folder() {
            // 创建工作区结构（如果不存在）
            let _ = workspace::create_workspace_structure(&folder);
            
            // 更新当前工作区路径
            self.workspace_dir = Some(folder.clone());
            
            // 扫描已完成的片段
            let mut completed_segments = Vec::new();
            for (i, segment) in self.audio_segments.iter().enumerate() {
                let srt_path = segment.with_extension("srt");
                if srt_path.exists() {
                    completed_segments.push(i);
                }
            }
            
            let state = workspace::WorkspaceState {
                video_path: self.video_path.clone(),
                audio_path: self.audio_path.clone(),
                cut_points: self.cut_points.clone(),
                audio_segments: self.audio_segments.clone(),
                completed_segments,  // 保存已完成的片段信息
                manual_segment: self.manual_segment.clone(),
                manual_start_time: self.manual_start_time.clone(),
                manual_end_time: self.manual_end_time.clone(),
                total_duration: self.total_duration,
                workspace_dir: folder.clone(),
            };
            
            match state.save(&folder) {
                Ok(_) => {
                    self.status_message = format!("Workspace saved to: {:?}", folder);
                }
                Err(e) => {
                    self.status_message = format!("Failed to save workspace: {}", e);
                }
            }
        }
    }
    
    fn check_missing_subtitles(&mut self) {
        self.missing_segments.clear();
        self.completed_segments.clear();
        self.can_resume = false;
        
        if self.audio_segments.is_empty() {
            return;
        }
        
        // 检查每个片段是否有对应的 SRT 文件
        for (i, segment) in self.audio_segments.iter().enumerate() {
            let srt_path = segment.with_extension("srt");
            if srt_path.exists() {
                self.completed_segments.push(i);
            } else {
                self.missing_segments.push(i);
            }
        }
        
        // 如果有缺失且有完成的，说明可以恢复
        self.can_resume = !self.missing_segments.is_empty() 
            && !self.completed_segments.is_empty();
    }
    
    fn resume_recognition(&mut self) {
        if self.missing_segments.is_empty() {
            self.status_message = "No missing segments to recognize!".to_string();
            return;
        }
        
        self.state = AppState::Processing;
        
        // 设置初始进度为已完成的百分比
        let completed_count = self.audio_segments.len() - self.missing_segments.len();
        self.processing_progress = completed_count as f32 / self.audio_segments.len() as f32;
        self.processing_status = format!("Resuming from {}/{} segments...", completed_count, self.audio_segments.len());
        self.recognition_results.clear();
        
        let segments: Vec<_> = self.missing_segments.iter()
            .filter_map(|&i| self.audio_segments.get(i).cloned())
            .collect();
        let missing_indices = self.missing_segments.clone();
        let all_segments = self.audio_segments.clone();
        let model = self.whisper_model;
        let language = self.whisper_language.clone();
        let custom_lang = self.custom_language_code.clone();
        let cut_points = self.cut_points.clone();
        let video_path = self.video_path.clone().unwrap();
        
        // 创建消息通道
        let (tx, rx) = channel();
        self.progress_receiver = Some(rx);
        
        std::thread::spawn(move || {
            let total_segments = all_segments.len();
            let completed_count = total_segments - missing_indices.len();  // 已完成的数量
            
            for (idx, segment) in segments.iter().enumerate() {
                let segment_index = missing_indices[idx];
                
                // 确定要使用的语言代码
                let lang_code = match language {
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
                        if custom_lang.is_empty() {
                            None
                        } else {
                            Some(custom_lang.as_str())
                        }
                    }
                };
                
                // 使用新的实时输出版本
                match whisper::recognize_audio_realtime(segment, model, lang_code, tx.clone(), segment_index + 1, total_segments) {
                    Ok((_srt_path, text)) => {
                        let _ = tx.send(ProgressMessage::Result { 
                            segment: segment_index + 1, 
                            text 
                        });
                        // 发送进度（识别完成后）- 包含已完成的数量
                        let _ = tx.send(ProgressMessage::Progress { 
                            current: completed_count + idx + 1,  // 已完成 + 当前进度
                            total: total_segments  // 总片段数
                        });
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to recognize segment {}: {}", segment_index + 1, e);
                        eprintln!("{}", error_msg);
                        let _ = tx.send(ProgressMessage::Error(error_msg));
                    }
                }
            }
            
            // 合并所有字幕
            let mut srt_files = Vec::new();
            for seg in &all_segments {
                let srt = seg.with_extension("srt");
                if srt.exists() {
                    srt_files.push(srt);
                }
            }
            
            if !srt_files.is_empty() {
                let output_path = video_path.with_extension("srt");
                match srt_merger::merge_srt_files(&srt_files, &cut_points, &output_path) {
                    Ok(_) => {
                        println!("Subtitles merged successfully: {:?}", output_path);
                    }
                    Err(e) => {
                        eprintln!("Failed to merge subtitles: {}", e);
                        let _ = tx.send(ProgressMessage::Error(format!("Failed to merge: {}", e)));
                    }
                }
            }
            
            // 发送完成消息
            let _ = tx.send(ProgressMessage::Completed);
        });
    }
}

impl eframe::App for WhisperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 处理识别进度消息
        let mut should_complete = false;
        if let Some(rx) = &self.progress_receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    ProgressMessage::Progress { current, total } => {
                        self.processing_status = format!("Recognizing segment {}/{}", current, total);
                        self.processing_progress = current as f32 / total as f32;
                    }
                    ProgressMessage::Result { segment, text } => {
                        let result = format!("Segment {}: {}", segment, text);
                        self.recognition_results.push(result);
                    }
                    ProgressMessage::RealtimeOutput(output) => {
                        // 实时输出信息
                        self.recognition_results.push(output);
                    }
                    ProgressMessage::Completed => {
                        should_complete = true;
                    }
                    ProgressMessage::Error(err) => {
                        self.recognition_results.push(format!("❌ Error: {}", err));
                    }
                }
            }
        }
        
        if should_complete {
            self.state = AppState::AudioExtracted;
            self.status_message = "Recognition completed!".to_string();
            self.progress_receiver = None;
        }
        
        // Update current playback position
        if let Some(player) = &self.audio_player {
            self.current_position = player.position();
        }
        
        // Handle dropped files
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(file) = i.raw.dropped_files.first() {
                    if let Some(path) = &file.path {
                        self.handle_dropped_file(path.clone());
                    }
                }
            }
        });
        
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Whisper Speech Recognition");
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 只要有视频加载就显示保存按钮
                    if self.video_path.is_some() {
                        if ui.button("💾 Save Workspace").clicked() {
                            self.save_workspace();
                        }
                    }
                    
                    if ui.button("📁 Open Folder").clicked() {
                        self.open_workspace();
                    }
                    
                    // Resume 按钮（在加载工作区后，如果有缺失的字幕）
                    if self.can_resume && self.state != AppState::Processing {
                        if ui.button("▶️ Resume").clicked() {
                            self.resume_recognition();
                        }
                    }
                });
            });
            ui.separator();
            
            ui.horizontal(|ui| {
                // Left panel: Drop area and player
                ui.vertical(|ui| {
                    ui.set_width(700.0);
                    
                    // Drop area
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgb(40, 40, 50))
                        .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 100, 120)))
                        .inner_margin(20.0)
                        .show(ui, |ui| {
                            ui.set_min_height(100.0);
                            ui.vertical_centered(|ui| {
                                if let Some(path) = &self.video_path {
                                    ui.label(format!("📹 {}", path.file_name().unwrap().to_string_lossy()));
                                } else {
                                    ui.label("📂 Drag video file here");
                                }
                            });
                        });
                    
                    ui.add_space(10.0);
                    
                    // Audio player
                    if self.state != AppState::Idle {
                        egui::Frame::default()
                            .fill(egui::Color32::from_rgb(30, 30, 40))
                            .inner_margin(15.0)
                            .show(ui, |ui| {
                                ui.label("🎵 Audio Player");
                                ui.separator();
                                
                                // Time display
                                ui.horizontal(|ui| {
                                    ui.label(Self::format_time(self.current_position));
                                    ui.label("/");
                                    ui.label(Self::format_time(self.total_duration));
                                });
                                
                                ui.add_space(5.0);
                                
                                // Time scale (5-minute intervals)
                                ui.horizontal(|ui| {
                                    let interval = 300.0; // 5 minutes in seconds
                                    let num_intervals = (self.total_duration / interval).ceil() as usize;
                                    
                                    for i in 0..=num_intervals {
                                        let time = i as f64 * interval;
                                        if time > self.total_duration {
                                            break;
                                        }
                                        
                                        let minutes = (time / 60.0).floor() as u32;
                                        let button_text = format!("{}m", minutes);
                                        
                                        if ui.small_button(&button_text).clicked() {
                                            self.current_position = time;
                                            if let Some(player) = &mut self.audio_player {
                                                player.seek(time);
                                                player.pause();
                                                self.is_playing = false;
                                            }
                                        }
                                        
                                        // 在按钮之间添加间隔
                                        if i < num_intervals {
                                            ui.add_space(3.0);
                                        }
                                    }
                                });
                                
                                // Playback progress bar (full width)
                                ui.add_space(5.0);
                                let mut position = self.current_position;
                                // 使用进度条宽度等于左侧面板宽度减去边距
                                ui.spacing_mut().slider_width = 640.0;
                                if ui.add(egui::Slider::new(&mut position, 0.0..=self.total_duration)
                                    .show_value(false)).changed() {
                                    self.current_position = position;
                                    if let Some(player) = &mut self.audio_player {
                                        player.seek(position);
                                    }
                                }
                                ui.add_space(5.0);
                                
                                ui.horizontal(|ui| {
                                    // Play/Pause button
                                    if self.is_playing {
                                        if ui.button("⏸ Pause").clicked() {
                                            if let Some(player) = &mut self.audio_player {
                                                player.pause();
                                                self.is_playing = false;
                                            }
                                        }
                                    } else {
                                        if ui.button("▶ Play").clicked() {
                                            if let Some(player) = &mut self.audio_player {
                                                player.play();
                                                self.is_playing = true;
                                            }
                                        }
                                    }
                                    
                                    // Mark cut point button
                                    if ui.button("✂ Mark Cut Point").clicked() {
                                        self.add_cut_point();
                                    }
                                });
                                
                                // Cut points list
                                if !self.cut_points.is_empty() {
                                    ui.separator();
                                    ui.label(format!("Cut Points ({}):", self.cut_points.len()));
                                    
                                    egui::ScrollArea::vertical()
                                        .max_height(150.0)
                                        .show(ui, |ui| {
                                            let mut to_remove = None;
                                            for (i, &point) in self.cut_points.iter().enumerate() {
                                                ui.horizontal(|ui| {
                                                    ui.label(format!("{}. {}", i + 1, Self::format_time(point)));
                                                    if ui.small_button("🗑").clicked() {
                                                        to_remove = Some(i);
                                                    }
                                                });
                                            }
                                            if let Some(i) = to_remove {
                                                self.remove_cut_point(i);
                                            }
                                        });
                                    
                                    ui.add_space(5.0);
                                    if ui.button("🔪 Execute Cut").clicked() {
                                        self.cut_audio();
                                    }
                                }
                            });
                    }
                    
                    ui.add_space(10.0);
                    
                    // Re-recognize section
                    if !self.audio_segments.is_empty() && self.state != AppState::Processing {
                        ui.separator();
                        ui.label("🔄 Re-recognize Segment");
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_label("Select segment")
                                .selected_text(format!("Segment {}", self.selected_segment_index + 1))
                                .show_ui(ui, |ui| {
                                    for i in 0..self.audio_segments.len() {
                                        ui.selectable_value(&mut self.selected_segment_index, i, format!("Segment {}", i + 1));
                                    }
                                });
                            
                            if ui.button("🎤 Re-recognize").clicked() {
                                self.rerecognize_segment();
                            }
                        });
                        
                        ui.add_space(5.0);
                        
                        // Cleanup button
                        if ui.button("🗑️ Clean Up Temp Files").clicked() {
                            self.cleanup_temp_files();
                        }
                    }
                    
                    ui.add_space(10.0);
                    
                    // Manual cut section
                    if self.state != AppState::Idle && self.state != AppState::Processing {
                        ui.separator();
                        ui.label("✂️ Manual Cut Segment");
                        
                        ui.horizontal(|ui| {
                            ui.label("Start:");
                            ui.text_edit_singleline(&mut self.manual_start_time);
                            ui.label("End:");
                            ui.text_edit_singleline(&mut self.manual_end_time);
                        });
                        ui.label("💡 Format: HH:MM:SS or MM:SS or SS");
                        
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            if ui.button("✂️ Cut Segment").clicked() {
                                self.cut_manual_segment();
                            }
                            
                            if self.manual_segment.is_some() {
                                if ui.button("🎤 Recognize Segment").clicked() {
                                    self.recognize_manual_segment();
                                }
                            }
                        });
                    }
                    
                    ui.add_space(10.0);
                    
                    // Status message
                    ui.label(&self.status_message);
                });
                
                ui.separator();
                
                // Right panel: Settings
                ui.vertical(|ui| {
                    ui.set_width(400.0);
                    
                    ui.heading("Settings");
                    ui.separator();
                    
                    // Whisper model selection
                    ui.label("Whisper Model:");
                    egui::ComboBox::from_label("")
                        .selected_text(self.whisper_model.as_str())
                        .show_ui(ui, |ui| {
                            for model in WhisperModel::all() {
                                ui.selectable_value(&mut self.whisper_model, model, model.as_str());
                            }
                        });
                    
                    ui.add_space(10.0);
                    
                    // Language selection
                    ui.label("Recognition Language:");
                    egui::ComboBox::from_label(" ")
                        .selected_text(self.whisper_language.as_str())
                        .show_ui(ui, |ui| {
                            for lang in WhisperLanguage::all() {
                                ui.selectable_value(&mut self.whisper_language, lang.clone(), lang.as_str());
                            }
                        });
                    
                    // Custom language input (only show when Custom is selected)
                    if self.whisper_language == WhisperLanguage::Custom {
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            ui.label("Language code:");
                            ui.text_edit_singleline(&mut self.custom_language_code);
                        });
                        ui.label("💡 Examples: ko (Korean), ar (Arabic), hi (Hindi), pt (Portuguese)");
                    }
                    
                    ui.add_space(20.0);
                    ui.separator();
                    
                    // Recognition section
                    ui.label("🎤 Recognition");
                    ui.add_space(5.0);
                    
                    if !self.audio_segments.is_empty() {
                        ui.label(format!("✅ Audio segments: {}", self.audio_segments.len()));
                        ui.add_space(10.0);
                        
                        if self.state != AppState::Processing {
                            if ui.button("🎤 Start Recognition").clicked() {
                                self.start_recognition();
                            }
                        } else {
                            ui.label("🔄 Recognizing...");
                            ui.label(&self.processing_status);
                            ui.add_space(5.0);
                            ui.add(egui::ProgressBar::new(self.processing_progress).show_percentage());
                            ui.add_space(5.0);
                            if ui.button("🛑 Stop Recognition & Kill Processes").clicked() {
                                self.stop_recognition();
                            }
                        }
                        
                        ui.add_space(10.0);
                        
                        // Recognition results
                        if !self.recognition_results.is_empty() {
                            ui.label("📝 Results:");
                            ui.add_space(5.0);
                            
                            egui::ScrollArea::vertical()
                                .max_height(180.0)
                                .show(ui, |ui| {
                                    for result in &self.recognition_results {
                                        egui::Frame::default()
                                            .fill(egui::Color32::from_rgb(35, 35, 45))
                                            .inner_margin(8.0)
                                            .show(ui, |ui| {
                                                ui.label(result);
                                            });
                                        ui.add_space(5.0);
                                    }
                                });
                        }
                    } else {
                        ui.label("⚠️ Please cut audio first");
                    }
                    
                    ui.add_space(10.0);
                    
                    // Save Plain Text button
                    if !self.recognition_results.is_empty() {
                        ui.separator();
                        if ui.button("💾 Save Plain Text").clicked() {
                            self.save_plain_text();
                        }
                    }
                });
            });
        });
        
        // Continuously refresh UI to update playback position
        ctx.request_repaint();
    }
}

