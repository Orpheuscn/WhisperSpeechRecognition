use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;
use anyhow::{Result, anyhow};

/// 视频/音频播放器（使用ffplay）
pub struct VideoPlayer {
    process: Option<Child>,
    media_path: PathBuf,
    duration: f64,
    start_time: Option<Instant>,
    start_position: f64,  // 播放开始时的位置（秒）
    is_paused: bool,
    pause_position: f64,  // 暂停时的位置
}

impl VideoPlayer {
    /// 创建新的播放器并获取时长
    pub fn new(media_path: &Path) -> Result<Self> {
        let duration = Self::get_duration(media_path)?;
        
        Ok(VideoPlayer {
            process: None,
            media_path: media_path.to_path_buf(),
            duration,
            start_time: None,
            start_position: 0.0,
            is_paused: true,
            pause_position: 0.0,
        })
    }
    
    /// 获取媒体文件时长
    fn get_duration(media_path: &Path) -> Result<f64> {
        let output = Command::new("ffprobe")
            .arg("-v")
            .arg("error")
            .arg("-show_entries")
            .arg("format=duration")
            .arg("-of")
            .arg("default=noprint_wrappers=1:nokey=1")
            .arg(media_path)
            .output()?;
        
        if !output.status.success() {
            return Err(anyhow!("获取时长失败"));
        }
        
        let duration_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let duration: f64 = duration_str.parse()
            .map_err(|_| anyhow!("解析时长失败"))?;
        
        Ok(duration)
    }
    
    /// 播放（从指定位置开始）
    pub fn play(&mut self) -> Result<()> {
        self.stop();
        
        let start_pos = if self.is_paused {
            self.pause_position
        } else {
            0.0
        };
        
        let mut cmd = Command::new("ffplay");
        
        // 如果指定了起始位置
        if start_pos > 0.0 {
            cmd.arg("-ss").arg(start_pos.to_string());
        }
        
        cmd.arg("-i").arg(&self.media_path);
        
        // ffplay选项
        cmd.arg("-autoexit")        // 播放完自动退出
           .arg("-hide_banner")      // 隐藏banner  
           .arg("-loglevel")
           .arg("quiet")             // 静默模式
           .arg("-nodisp");          // 不显示窗口（仅音频预览）
        
        // 启动进程
        cmd.stdout(Stdio::null())
           .stderr(Stdio::null());
        
        let child = cmd.spawn()?;
        self.process = Some(child);
        self.start_time = Some(Instant::now());
        self.start_position = start_pos;
        self.is_paused = false;
        
        Ok(())
    }
    
    /// 播放并显示视频（带字幕）
    pub fn play_with_video(&mut self, subtitle_path: Option<&Path>, start_pos: Option<f64>) -> Result<()> {
        self.stop();
        
        let start = start_pos.unwrap_or(0.0);
        
        let mut cmd = Command::new("ffplay");
        
        if start > 0.0 {
            cmd.arg("-ss").arg(start.to_string());
        }
        
        cmd.arg("-i").arg(&self.media_path);
        
        // 如果有字幕文件
        if let Some(srt_path) = subtitle_path {
            let srt_str = srt_path.to_string_lossy();
            let escaped_path = srt_str.replace("\\", "\\\\").replace(":", "\\:");
            let filter = format!("subtitles='{}'", escaped_path);
            cmd.arg("-vf").arg(filter);
        }
        
        cmd.arg("-autoexit")
           .arg("-hide_banner")
           .arg("-loglevel")
           .arg("quiet");
        
        cmd.stdout(Stdio::null())
           .stderr(Stdio::null());
        
        let child = cmd.spawn()?;
        self.process = Some(child);
        self.start_time = Some(Instant::now());
        self.start_position = start;
        self.is_paused = false;
        
        Ok(())
    }
    
    /// 暂停
    pub fn pause(&mut self) {
        if !self.is_paused && self.is_playing() {
            self.pause_position = self.position();
            self.stop();
            self.is_paused = true;
        }
    }
    
    /// 停止
    pub fn stop(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.start_time = None;
    }
    
    /// Seek到指定位置
    pub fn seek(&mut self, position: f64) {
        let was_playing = self.is_playing();
        self.pause_position = position.max(0.0).min(self.duration);
        self.is_paused = true;
        
        if was_playing {
            let _ = self.play();
        }
    }
    
    /// 获取当前播放位置
    pub fn position(&self) -> f64 {
        if self.is_paused {
            return self.pause_position;
        }
        
        if let Some(start_time) = self.start_time {
            let elapsed = start_time.elapsed().as_secs_f64();
            let pos = self.start_position + elapsed;
            pos.min(self.duration)
        } else {
            self.pause_position
        }
    }
    
    /// 获取总时长
    pub fn duration(&self) -> f64 {
        self.duration
    }
    
    /// 检查是否正在播放
    pub fn is_playing(&mut self) -> bool {
        if let Some(child) = &mut self.process {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // 进程已结束
                    self.process = None;
                    self.is_paused = true;
                    self.pause_position = self.duration;
                    false
                }
                Ok(None) => {
                    // 进程还在运行
                    true
                }
                Err(_) => {
                    self.process = None;
                    self.is_paused = true;
                    false
                }
            }
        } else {
            false
        }
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}
