use eframe::egui;
use crate::app_state::{WhisperApp, AppState, WhisperModel, WhisperLanguage, RecognitionMode};

impl WhisperApp {
    /// 渲染顶部工具栏
    pub fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Whisper视频字幕编辑器");
            
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.video_path.is_some() {
                    if ui.button("💾 保存工作区").clicked() {
                        self.save_workspace();
                    }
                }
                
                if ui.button("📁 打开文件夹").clicked() {
                    self.open_workspace();
                }
                
                if ui.button("📄 打开字幕").clicked() {
                    self.open_subtitle_file();
                }
                
                if self.can_resume && self.state != AppState::Processing {
                    if ui.button("▶️ 恢复").clicked() {
                        self.resume_recognition();
                    }
                }
            });
        });
    }
    
    /// 渲染文件加载区
    pub fn render_file_area(&self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(egui::Color32::from_rgb(40, 40, 50))
            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 100, 120)))
            .inner_margin(20.0)
            .show(ui, |ui| {
                ui.set_min_height(80.0);
                ui.vertical_centered(|ui| {
                    if let Some(path) = &self.video_path {
                        ui.label(format!("📹 {}", path.file_name().unwrap().to_string_lossy()));
                    } else {
                        ui.label("📂 拖拽视频或音频文件到此处");
                    }
                });
            });
    }
    
    /// 渲染音频播放器
    pub fn render_audio_player(&mut self, ui: &mut egui::Ui) {
        if self.state == AppState::Idle {
            return;
        }
        
        egui::Frame::default()
            .fill(egui::Color32::from_rgb(30, 30, 40))
            .inner_margin(15.0)
            .show(ui, |ui| {
                ui.label("🎵 音频播放器");
                ui.separator();
                
                ui.horizontal(|ui| {
                    ui.label(Self::format_time(self.current_position));
                    ui.label("/");
                    ui.label(Self::format_time(self.total_duration));
                });
                
                ui.add_space(5.0);
                
                // 时间刻度
                ui.horizontal(|ui| {
                    let interval = 300.0;
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
                            if let Some(player) = &mut self.video_player {
                                player.seek(time);
                                self.is_playing = false;
                            }
                        }
                        
                        if i < num_intervals {
                            ui.add_space(3.0);
                        }
                    }
                });
                
                // 进度条
                ui.add_space(5.0);
                let mut position = self.current_position;
                ui.spacing_mut().slider_width = 640.0;
                if ui.add(egui::Slider::new(&mut position, 0.0..=self.total_duration)
                    .show_value(false)).changed() {
                    self.current_position = position;
                    if let Some(player) = &mut self.video_player {
                        player.seek(position);
                    }
                }
                ui.add_space(5.0);
                
                ui.horizontal(|ui| {
                    if self.is_playing {
                        if ui.button("⏸ 暂停").clicked() {
                            if let Some(player) = &mut self.video_player {
                                player.pause();
                                self.is_playing = false;
                            }
                        }
                    } else {
                        if ui.button("▶ 播放").clicked() {
                            if let Some(player) = &mut self.video_player {
                                let _ = player.play();
                                self.is_playing = true;
                            }
                        }
                    }
                    
                    if ui.button("✂ 标记切割点").clicked() {
                        self.add_cut_point();
                    }
                });
                
                // 切割点列表
                if !self.cut_points.is_empty() {
                    ui.separator();
                    ui.label(format!("切割点 ({}):", self.cut_points.len()));
                    
                    egui::ScrollArea::vertical()
                        .max_height(100.0)
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
                    if ui.button("🔪 执行切割").clicked() {
                        self.cut_audio();
                    }
                }
            });
    }
    
    /// 渲染手动切割区域
    pub fn render_manual_cut(&mut self, ui: &mut egui::Ui) {
        if self.state == AppState::Idle || self.state == AppState::Processing {
            return;
        }
        
        ui.separator();
        ui.label("✂️ 手动切割片段");
        
        ui.horizontal(|ui| {
            ui.label("起始:");
            ui.text_edit_singleline(&mut self.manual_start_time);
            ui.label("结束:");
            ui.text_edit_singleline(&mut self.manual_end_time);
        });
        ui.label("💡 格式: HH:MM:SS.mmm 或 MM:SS.mmm 或 SS.mmm");
        
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            if ui.button("✂️ 切割片段").clicked() {
                self.cut_manual_segment();
            }
            
            if self.manual_segment.is_some() {
                if ui.button("🎤 识别片段").clicked() {
                    self.recognize_manual_segment();
                }
                
                if ui.button("🤖 VAD识别").clicked() {
                    self.recognition_mode = RecognitionMode::VAD;
                    self.recognize_manual_segment();
                    self.recognition_mode = RecognitionMode::Normal;
                }
            }
        });
    }
    
    /// 渲染设置面板
    pub fn render_settings_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.separator();
        
        // 模型选择
        ui.label("Whisper模型:");
        egui::ComboBox::from_label("")
            .selected_text(self.whisper_model.as_str())
            .show_ui(ui, |ui| {
                for model in WhisperModel::all() {
                    ui.selectable_value(&mut self.whisper_model, model, model.as_str());
                }
            });
        
        ui.add_space(10.0);
        
        // 语言选择
        ui.label("识别语言:");
        egui::ComboBox::from_label(" ")
            .selected_text(self.whisper_language.as_str())
            .show_ui(ui, |ui| {
                for lang in WhisperLanguage::all() {
                    ui.selectable_value(&mut self.whisper_language, lang.clone(), lang.as_str());
                }
            });
        
        if self.whisper_language == WhisperLanguage::Custom {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label("语言代码:");
                ui.text_edit_singleline(&mut self.custom_language_code);
            });
            ui.label("💡 示例: ko, ar, hi, pt");
        }
        
        ui.add_space(20.0);
        ui.separator();
        
        // 识别模式选择
        ui.label("🎤 识别模式");
        ui.add_space(5.0);
        
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.recognition_mode, RecognitionMode::Normal, "普通模式");
            ui.radio_value(&mut self.recognition_mode, RecognitionMode::VAD, "VAD模式");
        });
        
        ui.add_space(10.0);
        
        // 识别控制
        if !self.audio_segments.is_empty() {
            ui.label(format!("✅ 音频片段: {}", self.audio_segments.len()));
            ui.add_space(10.0);
            
            if self.state != AppState::Processing {
                if ui.button("🎤 开始识别").clicked() {
                    self.start_recognition();
                }
            } else {
                ui.label("🔄 识别中...");
                ui.label(&self.processing_status);
                ui.add_space(5.0);
                ui.add(egui::ProgressBar::new(self.processing_progress).show_percentage());
            }
            
            ui.add_space(10.0);
            
            // 识别结果
            if !self.recognition_results.is_empty() {
                ui.label("📝 结果:");
                ui.add_space(5.0);
                
                egui::ScrollArea::vertical()
                    .max_height(150.0)
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
            ui.label("⚠️ 请先切割音频");
        }
    }
    
    /// 渲染字幕编辑器
    pub fn render_subtitle_editor(&mut self, ui: &mut egui::Ui) {
        ui.heading("字幕编辑");
        ui.separator();
        
        if self.subtitles.is_empty() {
            ui.label("💡 识别完成后字幕会显示在这里");
            ui.label("   或点击上方的「📄 打开字幕」按钮加载字幕文件");
            return;
        }
        
        ui.horizontal(|ui| {
            ui.label(format!("共 {} 条字幕", self.subtitles.len()));
            
            if ui.button("💾 保存字幕").clicked() {
                self.save_subtitles();
            }
        });
        
        ui.add_space(5.0);
        
        egui::ScrollArea::vertical()
            .max_height(500.0)
            .show(ui, |ui| {
                let mut to_delete = None;
                
                for (idx, subtitle) in self.subtitles.iter_mut().enumerate() {
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgb(35, 35, 45))
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("#{}", subtitle.index));
                                ui.label(Self::format_time(subtitle.start_time));
                                ui.label("→");
                                ui.label(Self::format_time(subtitle.end_time));
                                
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("🗑").clicked() {
                                        to_delete = Some(idx);
                                    }
                                });
                            });
                            
                            ui.add_space(5.0);
                            
                            let text_edit = egui::TextEdit::multiline(&mut subtitle.text)
                                .desired_width(f32::INFINITY);
                            ui.add(text_edit);
                        });
                    
                    ui.add_space(5.0);
                }
                
                if let Some(idx) = to_delete {
                    self.delete_subtitle(idx);
                }
            });
    }
}

