mod ffmpeg;
mod whisper;
mod srt_merger;
mod recognition;
mod manual_cut;
mod workspace;

// 新增模块
mod app_state;
mod subtitle;
mod video_player;
mod vad_recognition;
mod fonts;
mod app_logic;
mod ui_components;

use eframe::egui;
use app_state::{WhisperApp, ProgressMessage};

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 800.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Whisper视频字幕编辑器",
        options,
        Box::new(|cc| {
            fonts::setup_fonts(&cc.egui_ctx);
            Ok(Box::new(WhisperApp::default()))
        }),
    )
}

impl eframe::App for WhisperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 处理识别进度消息
        let mut should_complete = false;
        if let Some(rx) = &self.progress_receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    ProgressMessage::Progress { current, total } => {
                        self.processing_status = format!("识别片段 {}/{}", current, total);
                        self.processing_progress = current as f32 / total as f32;
                    }
                    ProgressMessage::Result { segment, text } => {
                        let result = format!("片段 {}: {}", segment, text);
                        self.recognition_results.push(result);
                    }
                    ProgressMessage::RealtimeOutput(output) => {
                        self.recognition_results.push(output);
                    }
                    ProgressMessage::Completed => {
                        should_complete = true;
                        
                        // 识别完成后，尝试加载生成的字幕
                        if let Some(video_path) = &self.video_path {
                            let srt_path = video_path.with_extension("srt");
                            if srt_path.exists() {
                                if let Ok(subs) = subtitle::parse_srt_file(&srt_path) {
                                    self.subtitles = subs;
                                    self.srt_path = Some(srt_path);
                                }
                            }
                        }
                    }
                    ProgressMessage::Error(err) => {
                        self.recognition_results.push(format!("❌ 错误: {}", err));
                    }
                }
            }
        }
        
        if should_complete {
            self.state = app_state::AppState::AudioExtracted;
            self.status_message = "识别完成!".to_string();
            self.progress_receiver = None;
        }
        
        // 更新播放位置
        if let Some(player) = &mut self.video_player {
            self.current_position = player.position();
            
            // 检查播放状态
            if self.is_playing && !player.is_playing() {
                self.is_playing = false;
            }
        }
        
        // 处理拖拽文件
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(file) = i.raw.dropped_files.first() {
                    if let Some(path) = &file.path {
                        self.handle_dropped_file(path.clone());
                    }
                }
            }
        });
        
        // 主UI布局
        egui::CentralPanel::default().show(ctx, |ui| {
            // 顶部工具栏
            self.render_toolbar(ui);
            ui.separator();
            
            ui.horizontal(|ui| {
                // 左侧面板：文件加载 + 播放器 + 手动切割
                ui.vertical(|ui| {
                    ui.set_width(700.0);
                    
                    // 文件加载区
                    self.render_file_area(ui);
                    ui.add_space(10.0);
                    
                    // 音频播放器
                    self.render_audio_player(ui);
                    ui.add_space(10.0);
                    
                    // 手动切割
                    self.render_manual_cut(ui);
                    ui.add_space(10.0);
                    
                    // 状态消息
                    ui.label(&self.status_message);
                });
                
                ui.separator();
                
                // 右侧面板：设置 + 字幕编辑
                ui.vertical(|ui| {
                    ui.set_width(650.0);
                    
                    // 设置面板
                    self.render_settings_panel(ui);
                    
                    ui.add_space(20.0);
                    ui.separator();
                    
                    // 字幕编辑器
                    self.render_subtitle_editor(ui);
                });
            });
        });
        
        // 持续刷新UI
        ctx.request_repaint();
    }
}
