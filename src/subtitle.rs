use std::path::Path;
use std::fs;
use anyhow::{Result, anyhow};

/// 字幕条目
#[derive(Debug, Clone)]
pub struct SubtitleEntry {
    pub index: usize,
    pub start_time: f64,  // 秒
    pub end_time: f64,    // 秒
    pub text: String,
}

impl SubtitleEntry {
    pub fn new(index: usize, start_time: f64, end_time: f64, text: String) -> Self {
        SubtitleEntry {
            index,
            start_time,
            end_time,
            text,
        }
    }
    
    /// 转换为SRT格式的时间字符串
    pub fn format_srt_time(seconds: f64) -> String {
        let hours = (seconds / 3600.0).floor() as u32;
        let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
        let secs = (seconds % 60.0).floor() as u32;
        let millis = ((seconds % 1.0) * 1000.0).floor() as u32;
        
        format!("{:02}:{:02}:{:02},{:03}", hours, minutes, secs, millis)
    }
    
    /// 将时间字符串解析为秒数
    pub fn parse_srt_time(time_str: &str) -> Result<f64> {
        // 格式: HH:MM:SS,mmm
        let parts: Vec<&str> = time_str.split(&[':', ',']).collect();
        if parts.len() != 4 {
            return Err(anyhow!("Invalid SRT time format: {}", time_str));
        }
        
        let hours: f64 = parts[0].parse()?;
        let minutes: f64 = parts[1].parse()?;
        let seconds: f64 = parts[2].parse()?;
        let millis: f64 = parts[3].parse()?;
        
        Ok(hours * 3600.0 + minutes * 60.0 + seconds + millis / 1000.0)
    }
    
    /// 转换为SRT格式字符串
    pub fn to_srt_string(&self) -> String {
        format!(
            "{}\n{} --> {}\n{}\n\n",
            self.index,
            Self::format_srt_time(self.start_time),
            Self::format_srt_time(self.end_time),
            self.text
        )
    }
}

/// 解析SRT文件
pub fn parse_srt_file(path: &Path) -> Result<Vec<SubtitleEntry>> {
    let content = fs::read_to_string(path)?;
    parse_srt_content(&content)
}

/// 解析SRT内容
pub fn parse_srt_content(content: &str) -> Result<Vec<SubtitleEntry>> {
    let mut subtitles = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    
    let mut i = 0;
    while i < lines.len() {
        // 跳过空行
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        
        // 读取索引
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
        
        // 读取时间轴
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
        
        // 读取文本（可能多行）
        let mut text_lines = Vec::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            text_lines.push(lines[i].trim());
            i += 1;
        }
        
        let text = text_lines.join("\n");
        
        subtitles.push(SubtitleEntry::new(index, start_time, end_time, text));
    }
    
    Ok(subtitles)
}

/// 保存字幕到SRT文件
pub fn save_srt_file(path: &Path, subtitles: &[SubtitleEntry]) -> Result<()> {
    let mut content = String::new();
    
    for subtitle in subtitles {
        content.push_str(&subtitle.to_srt_string());
    }
    
    fs::write(path, content)?;
    Ok(())
}

/// 重新排列字幕索引
pub fn reindex_subtitles(subtitles: &mut [SubtitleEntry]) {
    for (i, subtitle) in subtitles.iter_mut().enumerate() {
        subtitle.index = i + 1;
    }
}

/// 按时间排序字幕
pub fn sort_subtitles_by_time(subtitles: &mut [SubtitleEntry]) {
    subtitles.sort_by(|a, b| {
        a.start_time.partial_cmp(&b.start_time).unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// 删除指定时间范围内的字幕
pub fn remove_subtitles_in_range(
    subtitles: &mut Vec<SubtitleEntry>,
    start_time: f64,
    end_time: f64,
) {
    subtitles.retain(|sub| {
        // 保留不在范围内的字幕
        sub.end_time <= start_time || sub.start_time >= end_time
    });
}

/// 插入新字幕到合适的位置
pub fn insert_subtitle(
    subtitles: &mut Vec<SubtitleEntry>,
    new_subtitle: SubtitleEntry,
) {
    // 找到合适的插入位置
    let insert_pos = subtitles
        .iter()
        .position(|sub| sub.start_time > new_subtitle.start_time)
        .unwrap_or(subtitles.len());
    
    subtitles.insert(insert_pos, new_subtitle);
}

/// 批量插入字幕
pub fn insert_subtitles(
    subtitles: &mut Vec<SubtitleEntry>,
    new_subtitles: Vec<SubtitleEntry>,
) {
    for subtitle in new_subtitles {
        insert_subtitle(subtitles, subtitle);
    }
    
    // 重新排序和编号
    sort_subtitles_by_time(subtitles);
    reindex_subtitles(subtitles);
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_srt_time() {
        assert_eq!(SubtitleEntry::parse_srt_time("00:00:05,650").unwrap(), 5.65);
        assert_eq!(SubtitleEntry::parse_srt_time("00:10:00,378").unwrap(), 600.378);
        assert_eq!(SubtitleEntry::parse_srt_time("01:30:45,123").unwrap(), 5445.123);
    }
    
    #[test]
    fn test_format_srt_time() {
        assert_eq!(SubtitleEntry::format_srt_time(5.65), "00:00:05,650");
        assert_eq!(SubtitleEntry::format_srt_time(600.378), "00:10:00,378");
        assert_eq!(SubtitleEntry::format_srt_time(5445.123), "01:30:45,123");
    }
}

