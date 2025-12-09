use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, anyhow};
use crate::ffmpeg;

/// 手动切割音频片段
/// 
/// 注意：切割后会将 WAV 片段转换为 MP3 格式，并删除 WAV 片段
pub fn cut_audio_segment(
    audio_path: &Path,
    start_time: f64,
    end_time: f64,
) -> Result<PathBuf> {
    if start_time >= end_time {
        return Err(anyhow!("Start time must be less than end time"));
    }
    
    let parent = audio_path.parent().unwrap();
    let stem = audio_path.file_stem().unwrap().to_string_lossy();
    let extension = audio_path.extension().unwrap().to_string_lossy();
    
    // 生成 WAV 输出文件名（临时）
    let wav_output_path = parent.join(format!("{}_manual_{:.2}_{:.2}.{}", 
        stem, start_time, end_time, extension));
    
    let duration = end_time - start_time;
    
    println!("🔪 手动切割音频片段 ({:.2}s - {:.2}s)...", start_time, end_time);
    
    let output = Command::new("ffmpeg")
        .arg("-i")
        .arg(audio_path)
        .arg("-ss")
        .arg(start_time.to_string())
        .arg("-t")
        .arg(duration.to_string())
        .arg("-acodec")
        .arg("copy")
        .arg("-y")
        .arg(&wav_output_path)
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to cut audio segment: {}", stderr));
    }
    
    // 转换为 MP3
    println!("🎵 转换片段为 MP3 格式...");
    let mp3_path = ffmpeg::convert_wav_to_mp3(&wav_output_path)?;
    println!("✅ 手动切割完成: {:?}", mp3_path);
    
    Ok(mp3_path)
}

/// 解析时间字符串（支持 HH:MM:SS.mmm 或 MM:SS.mmm 或 SS.mmm 或不带毫秒）
/// 
/// 支持的格式：
/// - SS (秒)
/// - SS.mmm (秒.毫秒)
/// - MM:SS (分:秒)
/// - MM:SS.mmm (分:秒.毫秒)
/// - HH:MM:SS (时:分:秒)
/// - HH:MM:SS.mmm (时:分:秒.毫秒)
pub fn parse_time_string(time_str: &str) -> Result<f64> {
    let parts: Vec<&str> = time_str.split(':').collect();
    
    let seconds = match parts.len() {
        1 => {
            // 只有秒（可能带毫秒）: SS 或 SS.mmm
            parts[0].parse::<f64>()?
        }
        2 => {
            // MM:SS 或 MM:SS.mmm
            let minutes: f64 = parts[0].parse()?;
            let seconds: f64 = parts[1].parse()?;
            minutes * 60.0 + seconds
        }
        3 => {
            // HH:MM:SS 或 HH:MM:SS.mmm
            let hours: f64 = parts[0].parse()?;
            let minutes: f64 = parts[1].parse()?;
            let seconds: f64 = parts[2].parse()?;
            hours * 3600.0 + minutes * 60.0 + seconds
        }
        _ => return Err(anyhow!("Invalid time format. Use HH:MM:SS.mmm, MM:SS.mmm, or SS.mmm"))
    };
    
    Ok(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_time_string() {
        // 不带毫秒
        assert_eq!(parse_time_string("30").unwrap(), 30.0);
        assert_eq!(parse_time_string("1:30").unwrap(), 90.0);
        assert_eq!(parse_time_string("1:30:45").unwrap(), 5445.0);
        
        // 带毫秒
        assert_eq!(parse_time_string("30.500").unwrap(), 30.5);
        assert_eq!(parse_time_string("1:30.250").unwrap(), 90.25);
        assert_eq!(parse_time_string("1:30:45.123").unwrap(), 5445.123);
        
        // 边界情况
        assert_eq!(parse_time_string("0:0:0.001").unwrap(), 0.001);
        assert_eq!(parse_time_string("0:0.1").unwrap(), 0.1);
    }
}

