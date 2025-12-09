use std::path::Path;
use std::process::{Child, Command, Stdio};
use anyhow::Result;

/// 视频播放器（使用ffplay）
pub struct VideoPlayer {
    process: Option<Child>,
    video_path: String,
}

impl VideoPlayer {
    pub fn new(video_path: &Path) -> Self {
        VideoPlayer {
            process: None,
            video_path: video_path.to_string_lossy().to_string(),
        }
    }
    
    /// 开始播放（带字幕）
    pub fn play_with_subtitle(&mut self, subtitle_path: Option<&Path>, start_time: Option<f64>) -> Result<()> {
        // 先停止当前播放
        self.stop();
        
        let mut cmd = Command::new("ffplay");
        
        // 输入文件
        cmd.arg("-i").arg(&self.video_path);
        
        // 如果指定了起始时间
        if let Some(time) = start_time {
            cmd.arg("-ss").arg(time.to_string());
        }
        
        // 如果有字幕文件，添加字幕滤镜
        if let Some(srt_path) = subtitle_path {
            let srt_str = srt_path.to_string_lossy();
            // 转义路径中的特殊字符
            let escaped_path = srt_str.replace("\\", "\\\\").replace(":", "\\:");
            let filter = format!("subtitles='{}'", escaped_path);
            cmd.arg("-vf").arg(filter);
        }
        
        // ffplay选项
        cmd.arg("-autoexit")        // 播放完自动退出
           .arg("-hide_banner")      // 隐藏banner
           .arg("-loglevel")
           .arg("quiet");            // 静默模式
        
        // 启动进程
        cmd.stdout(Stdio::null())
           .stderr(Stdio::null());
        
        let child = cmd.spawn()?;
        self.process = Some(child);
        
        Ok(())
    }
    
    /// 停止播放
    pub fn stop(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    
    /// 检查是否正在播放
    pub fn is_playing(&mut self) -> bool {
        if let Some(child) = &mut self.process {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // 进程已结束
                    self.process = None;
                    false
                }
                Ok(None) => {
                    // 进程还在运行
                    true
                }
                Err(_) => {
                    self.process = None;
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

