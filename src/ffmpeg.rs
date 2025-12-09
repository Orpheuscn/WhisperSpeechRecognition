use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use anyhow::{Result, anyhow};

/// 使用 FFmpeg 检测并提取音频
pub fn extract_audio(video_path: &Path) -> Result<PathBuf> {
    // 直接转换为 WAV 格式以确保最大兼容性
    let wav_path = video_path.with_extension("wav");
    
    let output = Command::new("ffmpeg")
        .arg("-i")
        .arg(video_path)
        .arg("-vn")            // 不处理视频
        .arg("-acodec")
        .arg("pcm_s16le")      // 转换为 WAV PCM 16-bit
        .arg("-ar")
        .arg("44100")          // 采样率 44.1kHz (标准音质)
        .arg("-ac")
        .arg("2")              // 立体声
        .arg("-y")             // 覆盖输出文件
        .arg(&wav_path)
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg failed to extract audio: {}", stderr));
    }
    
    Ok(wav_path)
}

/// 根据切割点切割音频文件
/// 
/// 注意：切割后会将 WAV 片段转换为 MP3 格式，并删除 WAV 片段
/// 完整的 WAV 文件会保留用于播放
pub fn cut_audio(audio_path: &Path, cut_points: &[f64]) -> Result<Vec<PathBuf>> {
    if cut_points.is_empty() {
        // 如果没有切割点，返回原始文件
        return Ok(vec![audio_path.to_path_buf()]);
    }
    
    let mut wav_segments = Vec::new();
    let mut start_time = 0.0;
    
    // 创建输出目录
    let parent = audio_path.parent().unwrap();
    let stem = audio_path.file_stem().unwrap().to_string_lossy();
    let extension = audio_path.extension().unwrap().to_string_lossy();
    
    println!("🔪 开始切割音频，共 {} 个切割点...", cut_points.len());
    
    // 根据切割点生成片段
    for (i, &cut_point) in cut_points.iter().enumerate() {
        let output_path = parent.join(format!("{}_{:03}.{}", stem, i, extension));
        
        let duration = cut_point - start_time;
        
        println!("   切割片段 {} ({:.2}s - {:.2}s)...", i + 1, start_time, cut_point);
        
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
            .arg(&output_path)
            .output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("切割音频失败: {}", stderr));
        }
        
        wav_segments.push(output_path);
        start_time = cut_point;
    }
    
    // 最后一段：从最后一个切割点到结束
    let output_path = parent.join(format!("{}_{:03}.{}", stem, cut_points.len(), extension));
    
    println!("   切割片段 {} ({:.2}s - 结束)...", cut_points.len() + 1, start_time);
    
    let output = Command::new("ffmpeg")
        .arg("-i")
        .arg(audio_path)
        .arg("-ss")
        .arg(start_time.to_string())
        .arg("-acodec")
        .arg("copy")
        .arg("-y")
        .arg(&output_path)
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("切割最后一段音频失败: {}", stderr));
    }
    
    wav_segments.push(output_path);
    
    // 将所有 WAV 片段转换为 MP3
    println!("🎵 转换片段为 MP3 格式...");
    let mut mp3_segments = Vec::new();
    
    for (i, wav_path) in wav_segments.iter().enumerate() {
        println!("   转换片段 {} 为 MP3...", i + 1);
        match convert_wav_to_mp3(wav_path) {
            Ok(mp3_path) => {
                mp3_segments.push(mp3_path);
                println!("   ✅ 片段 {} 转换完成", i + 1);
            }
            Err(e) => {
                eprintln!("   ❌ 片段 {} 转换失败: {}", i + 1, e);
                return Err(anyhow!("转换片段 {} 为 MP3 失败: {}", i + 1, e));
            }
        }
    }
    
    println!("✅ 音频切割和转换完成，共 {} 个 MP3 片段", mp3_segments.len());
    
    Ok(mp3_segments)
}

/// 将 WAV 音频文件转换为 MP3 格式
/// 
/// 参数：
/// - wav_path: WAV 文件路径
/// 
/// 返回：MP3 文件路径
/// 
/// 注意：转换完成后会删除原始 WAV 文件
pub fn convert_wav_to_mp3(wav_path: &Path) -> Result<PathBuf> {
    let mp3_path = wav_path.with_extension("mp3");
    
    // 使用 ffmpeg 转换为 MP3
    // 使用较高的比特率以保证质量
    let output = Command::new("ffmpeg")
        .arg("-i")
        .arg(wav_path)
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("192k")  // 192 kbps 比特率，平衡质量和文件大小
        .arg("-y")
        .arg(&mp3_path)
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("转换为 MP3 失败: {}", stderr));
    }
    
    // 验证 MP3 文件是否生成成功
    if !mp3_path.exists() {
        return Err(anyhow!("MP3 文件未生成"));
    }
    
    // 删除原始 WAV 文件
    if let Err(e) = fs::remove_file(wav_path) {
        eprintln!("警告: 删除 WAV 文件失败: {}", e);
        // 不返回错误，因为 MP3 已经生成成功
    }
    
    Ok(mp3_path)
}

/// 获取音频文件的时长
#[allow(dead_code)]
fn get_audio_duration(audio_path: &Path) -> Result<f64> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(audio_path)
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("获取音频时长失败"));
    }
    
    let duration_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let duration: f64 = duration_str.parse()?;
    
    Ok(duration)
}

