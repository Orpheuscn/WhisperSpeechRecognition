use std::path::Path;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use anyhow::{Result, anyhow};

#[derive(Debug, Clone)]
struct SubtitleEntry {
    index: usize,
    start_time: String,
    end_time: String,
    text: Vec<String>,
}

/// 解析 SRT 时间字符串为秒数
fn parse_srt_time(time_str: &str) -> Result<f64> {
    // 格式: HH:MM:SS,mmm
    let time_str = time_str.trim();
    
    if time_str.is_empty() {
        return Err(anyhow!("Empty time string"));
    }
    
    let parts: Vec<&str> = time_str.split(',').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid time format: {} (expected HH:MM:SS,mmm)", time_str));
    }
    
    let time_parts: Vec<&str> = parts[0].split(':').collect();
    if time_parts.len() != 3 {
        return Err(anyhow!("Invalid time format: {} (expected HH:MM:SS,mmm)", time_str));
    }
    
    let hours: f64 = time_parts[0].trim().parse()
        .map_err(|_| anyhow!("Invalid hour value: {}", time_parts[0]))?;
    let minutes: f64 = time_parts[1].trim().parse()
        .map_err(|_| anyhow!("Invalid minute value: {}", time_parts[1]))?;
    let seconds: f64 = time_parts[2].trim().parse()
        .map_err(|_| anyhow!("Invalid second value: {}", time_parts[2]))?;
    let milliseconds: f64 = parts[1].trim().parse()
        .map_err(|_| anyhow!("Invalid millisecond value: {}", parts[1]))?;
    
    Ok(hours * 3600.0 + minutes * 60.0 + seconds + milliseconds / 1000.0)
}

/// 将秒数转换为 SRT 时间格式
fn format_srt_time(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = (seconds % 60.0).floor() as u32;
    let millis = ((seconds % 1.0) * 1000.0).floor() as u32;
    
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, secs, millis)
}

/// 解析单个 SRT 文件
fn parse_srt_file(path: &Path) -> Result<Vec<SubtitleEntry>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    
    let mut lines = reader.lines();
    let mut current_entry: Option<SubtitleEntry> = None;
    
    while let Some(line) = lines.next() {
        let line = line?;
        let line = line.trim();
        
        if line.is_empty() {
            if let Some(entry) = current_entry.take() {
                // 只添加有效的条目（有时间和文本）
                if !entry.start_time.is_empty() && !entry.end_time.is_empty() {
                    entries.push(entry);
                }
            }
            continue;
        }
        
        // 尝试解析序号
        if let Ok(index) = line.parse::<usize>() {
            current_entry = Some(SubtitleEntry {
                index,
                start_time: String::new(),
                end_time: String::new(),
                text: Vec::new(),
            });
            continue;
        }
        
        // 尝试解析时间行
        if line.contains("-->") {
            let time_parts: Vec<&str> = line.split("-->").collect();
            if time_parts.len() == 2 {
                if let Some(ref mut entry) = current_entry {
                    let start = time_parts[0].trim();
                    let end = time_parts[1].trim();
                    // 验证时间格式非空
                    if !start.is_empty() && !end.is_empty() {
                        entry.start_time = start.to_string();
                        entry.end_time = end.to_string();
                    }
                }
            }
            continue;
        }
        
        // 字幕文本
        if let Some(ref mut entry) = current_entry {
            if !entry.start_time.is_empty() {
                entry.text.push(line.to_string());
            }
        }
    }
    
    // 添加最后一个条目
    if let Some(entry) = current_entry {
        if !entry.start_time.is_empty() && !entry.end_time.is_empty() {
            entries.push(entry);
        }
    }
    
    Ok(entries)
}

/// 合并多个 SRT 文件，根据切割点调整时间戳
pub fn merge_srt_files(
    srt_files: &[std::path::PathBuf],
    cut_points: &[f64],
    output_path: &Path,
) -> Result<()> {
    let mut merged_entries = Vec::new();
    let mut global_index = 1;
    
    // 计算每段的起始时间
    let mut segment_start_times = vec![0.0];
    segment_start_times.extend(cut_points.iter().copied());
    
    // 处理每个 SRT 文件
    for (segment_idx, srt_path) in srt_files.iter().enumerate() {
        let entries = parse_srt_file(srt_path)?;
        let time_offset = segment_start_times[segment_idx];
        
        for entry in entries {
            // 解析原始时间
            let start_seconds = parse_srt_time(&entry.start_time)?;
            let end_seconds = parse_srt_time(&entry.end_time)?;
            
            // 添加时间偏移
            let adjusted_start = start_seconds + time_offset;
            let adjusted_end = end_seconds + time_offset;
            
            // 创建新的条目
            merged_entries.push(SubtitleEntry {
                index: global_index,
                start_time: format_srt_time(adjusted_start),
                end_time: format_srt_time(adjusted_end),
                text: entry.text.clone(),
            });
            
            global_index += 1;
        }
    }
    
    // 按时间排序（以防万一）
    merged_entries.sort_by(|a, b| {
        let a_time = parse_srt_time(&a.start_time).unwrap_or(0.0);
        let b_time = parse_srt_time(&b.start_time).unwrap_or(0.0);
        a_time.partial_cmp(&b_time).unwrap()
    });
    
    // 重新编号
    for (i, entry) in merged_entries.iter_mut().enumerate() {
        entry.index = i + 1;
    }
    
    // 写入合并后的 SRT 文件
    let mut output_file = File::create(output_path)?;
    
    for entry in merged_entries {
        writeln!(output_file, "{}", entry.index)?;
        writeln!(output_file, "{} --> {}", entry.start_time, entry.end_time)?;
        for line in entry.text {
            writeln!(output_file, "{}", line)?;
        }
        writeln!(output_file)?;  // 空行
    }
    
    Ok(())
}
