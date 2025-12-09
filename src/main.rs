mod ffmpeg;
mod manual_cut;
mod subtitle;

use eframe::egui;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::sync::mpsc::{channel, Receiver};

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Whisper字幕生成器",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(App::default()))
        }),
    )
}

/// 设置中文字体
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    #[cfg(target_os = "macos")]
    {
        if let Ok(font_data) = std::fs::read("/System/Library/Fonts/PingFang.ttc") {
            fonts.font_data.insert(
                "pingfang".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            fonts.families.entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "pingfang".to_owned());
        }
    }
    
    ctx.set_fonts(fonts);
}

struct App {
    video_path: Option<PathBuf>,
    audio_path: Option<PathBuf>,
    status: String,
    
    // 手动切割
    start_time: String,
    end_time: String,
    
    // Whisper参数
    model: String,
    language: String,
    
    // 识别进度
    processing: bool,
    progress_rx: Option<Receiver<String>>,
    log_messages: Vec<String>,
    
    // 字幕
    subtitles: Vec<subtitle::SubtitleEntry>,
}

impl App {
    fn handle_drop(&mut self, path: PathBuf) {
        self.video_path = Some(path.clone());
        self.status = format!("已加载: {:?}", path.file_name().unwrap());
        self.audio_path = None;
        
        // 提取音频
        self.status = "正在提取音频...".to_string();
        match ffmpeg::extract_audio(&path) {
            Ok(audio) => {
                self.audio_path = Some(audio);
                self.status = "音频提取成功！".to_string();
            }
            Err(e) => {
                self.status = format!("提取失败: {}", e);
            }
        }
    }
    
    fn cut_and_recognize(&mut self) {
        let audio_path = match &self.audio_path {
            Some(p) => p.clone(),
            None => {
                self.status = "请先加载文件！".to_string();
                return;
            }
        };
        
        let start = match manual_cut::parse_time_string(&self.start_time) {
            Ok(t) => t,
            Err(_) => {
                self.status = "起始时间格式错误！".to_string();
                return;
            }
        };
        
        let end = match manual_cut::parse_time_string(&self.end_time) {
            Ok(t) => t,
            Err(_) => {
                self.status = "结束时间格式错误！".to_string();
                return;
            }
        };
        
        self.processing = true;
        self.log_messages.clear();
        
        let model = self.model.clone();
        let language = self.language.clone();
        
        let (tx, rx) = channel();
        self.progress_rx = Some(rx);
        
        std::thread::spawn(move || {
            // 切割片段
            let _ = tx.send(format!("正在切割音频片段 {:.1}s - {:.1}s...", start, end));
            
            match manual_cut::cut_audio_segment(&audio_path, start, end) {
                Ok(segment_path) => {
                    let _ = tx.send(format!("✅ 片段已切割: {:?}", segment_path));
                    
                    // 调用Python脚本识别
                    let _ = tx.send("正在启动VAD识别...".to_string());
                    
                    let script_path = "scripts/vad_transcribe_continuous.py";
                    
                    let mut cmd = Command::new("python3");
                    cmd.arg(script_path)
                       .arg(&segment_path)
                       .arg("--language").arg(&language)
                       .arg("--model").arg(&model)
                       .stdout(Stdio::piped())
                       .stderr(Stdio::piped());
                    
                    match cmd.spawn() {
                        Ok(mut child) => {
                            if let Some(stdout) = child.stdout.take() {
                                let reader = BufReader::new(stdout);
                                for line in reader.lines() {
                                    if let Ok(line) = line {
                                        let _ = tx.send(line);
                                    }
                                }
                            }
                            
                            match child.wait() {
                                Ok(status) if status.success() => {
                                    let _ = tx.send("✅ 识别完成！".to_string());
                                }
                                _ => {
                                    let _ = tx.send("❌ 识别失败！".to_string());
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(format!("❌ 启动失败: {}", e));
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(format!("❌ 切割失败: {}", e));
                }
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 处理进度消息
        let mut should_stop = false;
        let mut new_subtitles = None;
        
        if let Some(rx) = &self.progress_rx {
            while let Ok(msg) = rx.try_recv() {
                self.log_messages.push(msg.clone());
                
                if msg.contains("完成") || msg.contains("失败") {
                    should_stop = true;
                    
                    // 尝试加载生成的字幕
                    if let Some(video_path) = &self.video_path {
                        let srt_path = video_path.with_extension("srt");
                        if srt_path.exists() {
                            if let Ok(subs) = subtitle::parse_srt_file(&srt_path) {
                                new_subtitles = Some(subs);
                            }
                        }
                    }
                }
            }
        }
        
        if should_stop {
            self.processing = false;
            self.progress_rx = None;
            
            if let Some(subs) = new_subtitles {
                self.subtitles = subs;
                self.status = format!("字幕已生成: {} 条", self.subtitles.len());
            }
        }
        
        // 处理拖拽
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(file) = i.raw.dropped_files.first() {
                    if let Some(path) = &file.path {
                        self.handle_drop(path.clone());
                    }
                }
            }
        });
        
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Whisper字幕生成器");
            ui.separator();
            
            // 文件区域
            egui::Frame::default()
                .fill(egui::Color32::from_rgb(40, 40, 50))
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 100, 120)))
                .inner_margin(20.0)
                .show(ui, |ui| {
                    if let Some(path) = &self.video_path {
                        ui.label(format!("📹 {}", path.file_name().unwrap().to_string_lossy()));
                    } else {
                        ui.label("📂 拖拽视频或音频文件到此处");
                    }
                });
            
            ui.add_space(20.0);
            
            // 设置区
            ui.horizontal(|ui| {
                ui.label("模型:");
                egui::ComboBox::from_id_salt("model")
                    .selected_text(&self.model)
                    .show_ui(ui, |ui| {
                        for m in &["tiny", "base", "small", "medium", "large", "turbo"] {
                            ui.selectable_value(&mut self.model, m.to_string(), *m);
                        }
                    });
                
                ui.add_space(20.0);
                
                ui.label("语言:");
                egui::ComboBox::from_id_salt("lang")
                    .selected_text(&self.language)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.language, "Chinese".to_string(), "Chinese");
                        ui.selectable_value(&mut self.language, "Japanese".to_string(), "Japanese");
                        ui.selectable_value(&mut self.language, "English".to_string(), "English");
                    });
            });
            
            ui.add_space(20.0);
            ui.separator();
            
            // 手动切割区
            ui.label("✂️ 切割时间段");
            ui.horizontal(|ui| {
                ui.label("起始:");
                ui.text_edit_singleline(&mut self.start_time);
                ui.label("结束:");
                ui.text_edit_singleline(&mut self.end_time);
            });
            ui.label("💡 格式: HH:MM:SS.mmm 或 MM:SS 或 SS");
            
            ui.add_space(10.0);
            
            if !self.processing {
                if ui.button("🎤 切割并识别").clicked() {
                    self.cut_and_recognize();
                }
            } else {
                ui.label("🔄 识别中...");
            }
            
            ui.add_space(20.0);
            ui.separator();
            
            // 日志
            if !self.log_messages.is_empty() {
                ui.label("📝 日志:");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for msg in &self.log_messages {
                            ui.label(msg);
                        }
                    });
            }
            
            ui.add_space(10.0);
            
            // 状态
            ui.label(&self.status);
            
            // 字幕信息
            if !self.subtitles.is_empty() {
                ui.separator();
                ui.label(format!("✅ 字幕已生成: {} 条", self.subtitles.len()));
            }
        });
        
        ctx.request_repaint();
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            video_path: None,
            audio_path: None,
            status: String::new(),
            start_time: String::new(),
            end_time: String::new(),
            model: "base".to_string(),
            language: "Chinese".to_string(),
            processing: false,
            progress_rx: None,
            log_messages: Vec::new(),
            subtitles: Vec::new(),
        }
    }
}

