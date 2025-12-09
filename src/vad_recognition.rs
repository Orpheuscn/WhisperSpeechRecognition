use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::sync::mpsc::Sender;
use anyhow::{Result, anyhow};
use crate::app_state::{WhisperModel, WhisperLanguage, ProgressMessage};

/// 使用VAD python脚本进行识别
pub fn recognize_with_vad(
    audio_path: &Path,
    model: WhisperModel,
    language: &WhisperLanguage,
    custom_language: &str,
    tx: Sender<ProgressMessage>,
) -> Result<PathBuf> {
    // 获取语言代码
    let lang_code = language.to_code(custom_language)
        .ok_or_else(|| anyhow!("Language not specified"))?;
    
    // Python脚本路径
    let script_path = get_vad_script_path()?;
    
    // 输出SRT路径
    let output_srt = audio_path.with_extension("srt");
    
    println!("🔍 使用VAD模式识别...");
    println!("   音频: {:?}", audio_path);
    println!("   模型: {}", model.as_str());
    println!("   语言: {}", lang_code);
    
    // 发送开始消息
    let _ = tx.send(ProgressMessage::RealtimeOutput(
        "开始VAD语音检测...".to_string()
    ));
    
    // 构建命令
    let mut cmd = Command::new("python3");
    cmd.arg(&script_path)
       .arg(audio_path)
       .arg("--language")
       .arg(lang_code)
       .arg("--model")
       .arg(model.as_str())
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());
    
    // 启动进程
    let mut child = cmd.spawn()?;
    
    // 读取输出
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                println!("   VAD: {}", line);
                // 发送实时输出
                let _ = tx.send(ProgressMessage::RealtimeOutput(line));
            }
        }
    }
    
    // 等待完成
    let status = child.wait()?;
    
    if !status.success() {
        return Err(anyhow!("VAD识别失败"));
    }
    
    // 检查输出文件是否存在
    if !output_srt.exists() {
        return Err(anyhow!("SRT文件未生成"));
    }
    
    println!("✅ VAD识别完成: {:?}", output_srt);
    
    Ok(output_srt)
}

/// 获取VAD脚本路径
fn get_vad_script_path() -> Result<PathBuf> {
    // 尝试多个可能的位置
    let possible_paths = vec![
        PathBuf::from("scripts/vad_transcribe_continuous.py"),
        PathBuf::from("../scripts/vad_transcribe_continuous.py"),
        PathBuf::from("./vad_transcribe_continuous.py"),
    ];
    
    for path in possible_paths {
        if path.exists() {
            return Ok(path);
        }
    }
    
    Err(anyhow!("找不到VAD脚本文件"))
}

/// 使用VAD模式识别单个片段（用于Manual Cut）
pub fn recognize_segment_with_vad(
    audio_path: &Path,
    start_time: f64,
    _end_time: f64,
    model: WhisperModel,
    language: &WhisperLanguage,
    custom_language: &str,
    tx: Sender<ProgressMessage>,
) -> Result<Vec<crate::subtitle::SubtitleEntry>> {
    // 先用VAD识别整个片段
    let srt_path = recognize_with_vad(audio_path, model, language, custom_language, tx)?;
    
    // 解析SRT文件
    let mut subtitles = crate::subtitle::parse_srt_file(&srt_path)?;
    
    // 调整时间偏移（因为是从start_time开始的）
    for subtitle in &mut subtitles {
        subtitle.start_time += start_time;
        subtitle.end_time += start_time;
    }
    
    Ok(subtitles)
}

