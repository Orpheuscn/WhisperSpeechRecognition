use std::path::PathBuf;
use std::sync::mpsc::channel;
use crate::app_state::{WhisperApp, AppState, ProgressMessage, RecognitionMode};
use crate::{ffmpeg, whisper, srt_merger, manual_cut, workspace, subtitle, vad_recognition};

impl WhisperApp {
    /// 处理拖拽的文件
    pub fn handle_dropped_file(&mut self, path: PathBuf) {
        self.video_path = Some(path.clone());
        self.state = AppState::Idle;
        self.status_message = format!("文件已加载: {:?}", path.file_name().unwrap());
        self.audio_path = None;
        self.audio_player = None;
        self.cut_points.clear();
        self.audio_segments.clear();
        self.recognition_results.clear();
        self.workspace_dir = None;
        
        // 检查文件类型
        let extension = path.extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        
        if matches!(extension.as_str(), "wav" | "mp3" | "m4a" | "flac" | "ogg" | "opus") {
            self.load_audio_file(path);
        } else {
            self.extract_audio();
        }
    }
    
    /// 加载音频文件
    fn load_audio_file(&mut self, audio_path: PathBuf) {
        self.audio_path = Some(audio_path.clone());
        self.status_message = "音频文件已加载!".to_string();
        self.state = AppState::AudioExtracted;
        
        match crate::audio_player::AudioPlayer::new(&audio_path) {
            Ok(player) => {
                self.total_duration = player.duration();
                self.audio_player = Some(player);
            }
            Err(e) => {
                self.status_message = format!("加载音频失败: {}", e);
            }
        }
    }
    
    /// 提取音频
    fn extract_audio(&mut self) {
        if let Some(video_path) = &self.video_path {
            self.status_message = "正在提取音频...".to_string();
            
            match ffmpeg::extract_audio(video_path) {
                Ok(audio_path) => {
                    self.audio_path = Some(audio_path.clone());
                    self.status_message = "音频提取成功!".to_string();
                    self.state = AppState::AudioExtracted;
                    
                    match crate::audio_player::AudioPlayer::new(&audio_path) {
                        Ok(player) => {
                            self.total_duration = player.duration();
                            self.audio_player = Some(player);
                        }
                        Err(e) => {
                            self.status_message = format!("加载音频失败: {}", e);
                        }
                    }
                }
                Err(e) => {
                    self.status_message = format!("提取音频失败: {}", e);
                }
            }
        }
    }
    
    /// 添加切割点
    pub fn add_cut_point(&mut self) {
        if !self.cut_points.contains(&self.current_position) {
            self.cut_points.push(self.current_position);
            self.cut_points.sort_by(|a, b| a.partial_cmp(b).unwrap());
        }
    }
    
    /// 移除切割点
    pub fn remove_cut_point(&mut self, index: usize) {
        if index < self.cut_points.len() {
            self.cut_points.remove(index);
        }
    }
    
    /// 执行音频切割
    pub fn cut_audio(&mut self) {
        if let Some(audio_path) = &self.audio_path {
            self.status_message = "正在切割音频...".to_string();
            self.state = AppState::Processing;
            
            match ffmpeg::cut_audio(audio_path, &self.cut_points) {
                Ok(segments) => {
                    self.audio_segments = segments;
                    self.status_message = format!("音频切割完成，共 {} 个片段", self.audio_segments.len());
                    self.state = AppState::AudioExtracted;
                }
                Err(e) => {
                    self.status_message = format!("切割音频失败: {}", e);
                    self.state = AppState::AudioExtracted;
                }
            }
        }
    }
    
    /// 开始识别（普通模式或VAD模式）
    pub fn start_recognition(&mut self) {
        if self.audio_segments.is_empty() {
            self.status_message = "请先切割音频!".to_string();
            return;
        }
        
        self.state = AppState::Processing;
        self.processing_progress = 0.0;
        self.processing_status = "开始识别...".to_string();
        self.recognition_results.clear();
        
        match self.recognition_mode {
            RecognitionMode::Normal => self.start_normal_recognition(),
            RecognitionMode::VAD => self.start_vad_recognition(),
        }
    }
    
    /// 普通识别模式
    fn start_normal_recognition(&mut self) {
        let segments = self.audio_segments.clone();
        let model = self.whisper_model;
        let language = self.whisper_language.clone();
        let custom_lang = self.custom_language_code.clone();
        let cut_points = self.cut_points.clone();
        let video_path = self.video_path.clone().unwrap();
        
        let (tx, rx) = channel();
        self.progress_receiver = Some(rx);
        
        std::thread::spawn(move || {
            let total = segments.len();
            let mut srt_files = Vec::new();
            
            for (i, segment) in segments.iter().enumerate() {
                let lang_code = language.to_code(&custom_lang);
                
                match whisper::recognize_audio_realtime(segment, model, lang_code, tx.clone(), i + 1, total) {
                    Ok((srt_path, text)) => {
                        srt_files.push(srt_path);
                        let _ = tx.send(ProgressMessage::Result { segment: i + 1, text });
                        let _ = tx.send(ProgressMessage::Progress { current: i + 1, total });
                    }
                    Err(e) => {
                        let _ = tx.send(ProgressMessage::Error(format!("识别片段 {} 失败: {}", i + 1, e)));
                    }
                }
            }
            
            if !srt_files.is_empty() {
                let output_path = video_path.with_extension("srt");
                match srt_merger::merge_srt_files(&srt_files, &cut_points, &output_path) {
                    Ok(_) => println!("字幕合并成功: {:?}", output_path),
                    Err(e) => {
                        let _ = tx.send(ProgressMessage::Error(format!("合并失败: {}", e)));
                    }
                }
            }
            
            let _ = tx.send(ProgressMessage::Completed);
        });
    }
    
    /// VAD识别模式
    fn start_vad_recognition(&mut self) {
        let audio_path = self.audio_path.clone().unwrap();
        let model = self.whisper_model;
        let language = self.whisper_language.clone();
        let custom_lang = self.custom_language_code.clone();
        
        let (tx, rx) = channel();
        self.progress_receiver = Some(rx);
        
        std::thread::spawn(move || {
            match vad_recognition::recognize_with_vad(&audio_path, model, &language, &custom_lang, tx.clone()) {
                Ok(srt_path) => {
                    let _ = tx.send(ProgressMessage::RealtimeOutput(
                        format!("VAD识别完成: {:?}", srt_path)
                    ));
                }
                Err(e) => {
                    let _ = tx.send(ProgressMessage::Error(format!("VAD识别失败: {}", e)));
                }
            }
            let _ = tx.send(ProgressMessage::Completed);
        });
    }
    
    /// 手动切割片段
    pub fn cut_manual_segment(&mut self) {
        if let Some(audio_path) = &self.audio_path {
            let start_time = match manual_cut::parse_time_string(&self.manual_start_time) {
                Ok(t) => t,
                Err(_) => {
                    self.status_message = "起始时间格式无效!".to_string();
                    return;
                }
            };
            
            let end_time = match manual_cut::parse_time_string(&self.manual_end_time) {
                Ok(t) => t,
                Err(_) => {
                    self.status_message = "结束时间格式无效!".to_string();
                    return;
                }
            };
            
            match manual_cut::cut_audio_segment(audio_path, start_time, end_time) {
                Ok(segment_path) => {
                    self.manual_segment = Some(segment_path);
                    self.status_message = format!("手动片段已切割: {:.2}s - {:.2}s", start_time, end_time);
                }
                Err(e) => {
                    self.status_message = format!("切割片段失败: {}", e);
                }
            }
        }
    }
    
    /// 识别手动片段（支持VAD模式）
    pub fn recognize_manual_segment(&mut self) {
        if self.manual_segment.is_none() {
            self.status_message = "没有手动片段可识别!".to_string();
            return;
        }
        
        self.state = AppState::Processing;
        self.processing_progress = 0.0;
        self.processing_status = "正在识别手动片段...".to_string();
        self.recognition_results.clear();
        
        let segment = self.manual_segment.clone().unwrap();
        let model = self.whisper_model;
        let language = self.whisper_language.clone();
        let custom_lang = self.custom_language_code.clone();
        let recognition_mode = self.recognition_mode;
        
        let start_time = manual_cut::parse_time_string(&self.manual_start_time).unwrap_or(0.0);
        let end_time = manual_cut::parse_time_string(&self.manual_end_time).unwrap_or(0.0);
        
        let mut subtitles = self.subtitles.clone();
        
        let (tx, rx) = channel();
        self.progress_receiver = Some(rx);
        
        std::thread::spawn(move || {
            let new_subs = match recognition_mode {
                RecognitionMode::Normal => {
                    // 普通模式识别
                    match crate::recognition::recognize_single_segment(
                        &segment, 0, 1, model, &language, &custom_lang, tx.clone()
                    ) {
                        Ok((srt_path, _text)) => {
                            subtitle::parse_srt_file(&srt_path).unwrap_or_default()
                        }
                        Err(e) => {
                            let _ = tx.send(ProgressMessage::Error(format!("识别失败: {}", e)));
                            vec![]
                        }
                    }
                }
                RecognitionMode::VAD => {
                    // VAD模式识别
                    match vad_recognition::recognize_segment_with_vad(
                        &segment, start_time, end_time, model, &language, &custom_lang, tx.clone()
                    ) {
                        Ok(subs) => subs,
                        Err(e) => {
                            let _ = tx.send(ProgressMessage::Error(format!("VAD识别失败: {}", e)));
                            vec![]
                        }
                    }
                }
            };
            
            if !new_subs.is_empty() {
                // 移除时间范围内的旧字幕
                subtitle::remove_subtitles_in_range(&mut subtitles, start_time, end_time);
                
                // 插入新字幕
                subtitle::insert_subtitles(&mut subtitles, new_subs);
                
                let _ = tx.send(ProgressMessage::RealtimeOutput(
                    format!("已更新字幕：{:.2}s - {:.2}s", start_time, end_time)
                ));
            }
            
            let _ = tx.send(ProgressMessage::Completed);
        });
    }
    
    /// 保存工作区
    pub fn save_workspace(&mut self) {
        let default_dir = self.video_path.as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        
        let mut dialog = rfd::FileDialog::new();
        if let Some(dir) = default_dir {
            dialog = dialog.set_directory(dir);
        }
        
        if let Some(folder) = dialog.pick_folder() {
            let _ = workspace::create_workspace_structure(&folder);
            self.workspace_dir = Some(folder.clone());
            
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
                completed_segments,
                manual_segment: self.manual_segment.clone(),
                manual_start_time: self.manual_start_time.clone(),
                manual_end_time: self.manual_end_time.clone(),
                total_duration: self.total_duration,
                workspace_dir: folder.clone(),
            };
            
            match state.save(&folder) {
                Ok(_) => {
                    self.status_message = format!("工作区已保存到: {:?}", folder);
                }
                Err(e) => {
                    self.status_message = format!("保存工作区失败: {}", e);
                }
            }
        }
    }
    
    /// 打开工作区
    pub fn open_workspace(&mut self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            if workspace::WorkspaceState::exists(&folder) {
                match workspace::WorkspaceState::load(&folder) {
                    Ok(state) => {
                        self.workspace_dir = Some(folder.clone());
                        self.video_path = state.video_path.clone();
                        self.audio_path = state.audio_path.clone();
                        self.cut_points = state.cut_points.clone();
                        self.audio_segments = state.audio_segments.clone();
                        self.manual_segment = state.manual_segment.clone();
                        self.manual_start_time = state.manual_start_time.clone();
                        self.manual_end_time = state.manual_end_time.clone();
                        self.total_duration = state.total_duration;
                        
                        if let Some(audio_path) = &state.audio_path {
                            if audio_path.exists() {
                                match crate::audio_player::AudioPlayer::new(audio_path) {
                                    Ok(player) => {
                                        self.audio_player = Some(player);
                                        self.state = AppState::AudioExtracted;
                                    }
                                    Err(_) => {
                                        self.state = AppState::AudioExtracted;
                                    }
                                }
                            }
                        }
                        
                        self.check_missing_subtitles();
                        
                        let completed = self.completed_segments.len();
                        let total = self.audio_segments.len();
                        self.status_message = format!("工作区已加载! {}/{} 片段已完成.", completed, total);
                    }
                    Err(e) => {
                        self.status_message = format!("加载工作区失败: {}", e);
                    }
                }
            } else {
                match workspace::create_workspace_structure(&folder) {
                    Ok(_) => {
                        self.workspace_dir = Some(folder.clone());
                        self.status_message = format!("新工作区已创建: {:?}", folder);
                    }
                    Err(e) => {
                        self.status_message = format!("创建工作区失败: {}", e);
                    }
                }
            }
        }
    }
    
    /// 打开字幕文件
    pub fn open_subtitle_file(&mut self) {
        if let Some(file) = rfd::FileDialog::new()
            .add_filter("SRT字幕", &["srt"])
            .pick_file()
        {
            match subtitle::parse_srt_file(&file) {
                Ok(subs) => {
                    self.subtitles = subs;
                    self.srt_path = Some(file.clone());
                    self.status_message = format!("字幕已加载: {} 条", self.subtitles.len());
                }
                Err(e) => {
                    self.status_message = format!("加载字幕失败: {}", e);
                }
            }
        }
    }
    
    /// 保存字幕
    pub fn save_subtitles(&mut self) {
        if let Some(srt_path) = &self.srt_path {
            match subtitle::save_srt_file(srt_path, &self.subtitles) {
                Ok(_) => {
                    self.status_message = format!("字幕已保存: {} 条", self.subtitles.len());
                }
                Err(e) => {
                    self.status_message = format!("保存字幕失败: {}", e);
                }
            }
        } else if let Some(video_path) = &self.video_path {
            let srt_path = video_path.with_extension("srt");
            match subtitle::save_srt_file(&srt_path, &self.subtitles) {
                Ok(_) => {
                    self.srt_path = Some(srt_path.clone());
                    self.status_message = format!("字幕已保存: {:?}", srt_path);
                }
                Err(e) => {
                    self.status_message = format!("保存字幕失败: {}", e);
                }
            }
        }
    }
    
    /// 删除字幕
    pub fn delete_subtitle(&mut self, index: usize) {
        if index < self.subtitles.len() {
            self.subtitles.remove(index);
            subtitle::reindex_subtitles(&mut self.subtitles);
        }
    }
    
    /// 检查缺失的字幕
    fn check_missing_subtitles(&mut self) {
        self.missing_segments.clear();
        self.completed_segments.clear();
        self.can_resume = false;
        
        if self.audio_segments.is_empty() {
            return;
        }
        
        for (i, segment) in self.audio_segments.iter().enumerate() {
            let srt_path = segment.with_extension("srt");
            if srt_path.exists() {
                self.completed_segments.push(i);
            } else {
                self.missing_segments.push(i);
            }
        }
        
        self.can_resume = !self.missing_segments.is_empty() && !self.completed_segments.is_empty();
    }
    
    /// 恢复识别
    pub fn resume_recognition(&mut self) {
        if self.missing_segments.is_empty() {
            self.status_message = "没有缺失的片段需要识别!".to_string();
            return;
        }
        
        self.state = AppState::Processing;
        let completed_count = self.audio_segments.len() - self.missing_segments.len();
        self.processing_progress = completed_count as f32 / self.audio_segments.len() as f32;
        self.processing_status = format!("从 {}/{} 片段恢复...", completed_count, self.audio_segments.len());
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
        
        let (tx, rx) = channel();
        self.progress_receiver = Some(rx);
        
        std::thread::spawn(move || {
            let total_segments = all_segments.len();
            let completed_count = total_segments - missing_indices.len();
            
            for (idx, segment) in segments.iter().enumerate() {
                let segment_index = missing_indices[idx];
                let lang_code = language.to_code(&custom_lang);
                
                match whisper::recognize_audio_realtime(segment, model, lang_code, tx.clone(), segment_index + 1, total_segments) {
                    Ok((_srt_path, text)) => {
                        let _ = tx.send(ProgressMessage::Result { segment: segment_index + 1, text });
                        let _ = tx.send(ProgressMessage::Progress { 
                            current: completed_count + idx + 1, 
                            total: total_segments 
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(ProgressMessage::Error(format!("识别片段 {} 失败: {}", segment_index + 1, e)));
                    }
                }
            }
            
            let mut srt_files = Vec::new();
            for seg in &all_segments {
                let srt = seg.with_extension("srt");
                if srt.exists() {
                    srt_files.push(srt);
                }
            }
            
            if !srt_files.is_empty() {
                let output_path = video_path.with_extension("srt");
                let _ = srt_merger::merge_srt_files(&srt_files, &cut_points, &output_path);
            }
            
            let _ = tx.send(ProgressMessage::Completed);
        });
    }
}

