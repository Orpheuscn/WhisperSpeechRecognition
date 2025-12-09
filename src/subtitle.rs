use std::path::Path;
use std::fs;
use anyhow::{Result, anyhow};

/// 字幕条目
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SubtitleEntry {
    pub index: usize,
    pub start_time: f64,
    pub end_time: f64,
    pub text: String,
}

impl SubtitleEntry {
    /// 解析SRT时间字符串为秒数
    fn parse_srt_time(time_str: &str) -> Result<f64> {
        let parts: Vec<&str> = time_str.split(&[':', ',']).collect();
        if parts.len() != 4 {
            return Err(anyhow!("Invalid SRT time format"));
        }
        
        let hours: f64 = parts[0].parse()?;
        let minutes: f64 = parts[1].parse()?;
        let seconds: f64 = parts[2].parse()?;
        let millis: f64 = parts[3].parse()?;
        
        Ok(hours * 3600.0 + minutes * 60.0 + seconds + millis / 1000.0)
    }
}

/// 解析SRT文件
pub fn parse_srt_file(path: &Path) -> Result<Vec<SubtitleEntry>> {
    let content = fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    
    let mut subtitles = Vec::new();
    let mut i = 0;
    
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        
        let index: usize = match lines[i].trim().parse() {
            Ok(idx) => idx,
            Err(_) => {
                i += 1;
                continue;
            }
        };
        
        i += 1;
        if i >= lines.len() {
            break;
        }
        
        let time_line = lines[i].trim();
        if !time_line.contains("-->") {
            i += 1;
            continue;
        }
        
        let time_parts: Vec<&str> = time_line.split("-->").map(|s| s.trim()).collect();
        if time_parts.len() != 2 {
            i += 1;
            continue;
        }
        
        let start_time = SubtitleEntry::parse_srt_time(time_parts[0])?;
        let end_time = SubtitleEntry::parse_srt_time(time_parts[1])?;
        
        i += 1;
        
        let mut text_lines = Vec::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            text_lines.push(lines[i].trim());
            i += 1;
        }
        
        let text = text_lines.join("\n");
        
        subtitles.push(SubtitleEntry {
            index,
            start_time,
            end_time,
            text,
        });
    }
    
    Ok(subtitles)
}
