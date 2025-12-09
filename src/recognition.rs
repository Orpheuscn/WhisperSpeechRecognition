use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use anyhow::Result;
use crate::app_state::{WhisperModel, WhisperLanguage, ProgressMessage};
use crate::{srt_merger, whisper};

/// 识别单个音频片段
pub fn recognize_single_segment(
    segment_path: &Path,
    segment_index: usize,
    total_segments: usize,
    model: WhisperModel,
    language: &WhisperLanguage,
    custom_language: &str,
    tx: Sender<ProgressMessage>,
) -> Result<(PathBuf, String)> {
    // 确定要使用的语言代码
    let lang_code = match language {
        WhisperLanguage::Unknown => None,
        WhisperLanguage::Japanese => Some("ja"),
        WhisperLanguage::English => Some("en"),
        WhisperLanguage::Chinese => Some("zh"),
        WhisperLanguage::French => Some("fr"),
        WhisperLanguage::German => Some("de"),
        WhisperLanguage::Spanish => Some("es"),
        WhisperLanguage::Italian => Some("it"),
        WhisperLanguage::Russian => Some("ru"),
        WhisperLanguage::Custom => {
            if custom_language.is_empty() {
                None
            } else {
                Some(custom_language)
            }
        }
    };
    
    // 调用 whisper 识别
    whisper::recognize_audio_realtime(
        segment_path,
        model,
        lang_code,
        tx.clone(),
        segment_index + 1,
        total_segments,
    )
}

/// 重新合并所有字幕
pub fn remerge_subtitles(
    srt_files: &[PathBuf],
    cut_points: &[f64],
    output_path: &Path,
) -> Result<()> {
    srt_merger::merge_srt_files(srt_files, cut_points, output_path)
}

