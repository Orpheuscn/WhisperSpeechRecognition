#!/usr/bin/env python3
"""
连续语音识别脚本 - 针对对白稀疏的音频（改进版）
检测连续语音片段，分别识别后合并字幕
"""
import argparse
import subprocess
import os
import json
import time
from pathlib import Path
from typing import List, Tuple, Dict
import torch
import torchaudio


def extract_audio(input_file: str, output_audio: str) -> None:
    """从视频或音频中提取音频，ffmpeg会自动处理所有格式"""
    print(f"正在提取音频（ffmpeg自动检测格式）...")
    cmd = [
        'ffmpeg', '-i', input_file,
        '-vn', '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le',
        '-y', output_audio
    ]
    subprocess.run(cmd, check=True)


def detect_continuous_speech_segments(audio_file: str, 
                                      silence_threshold_sec: float = 2.0,
                                      speech_pad_ms: int = 300) -> List[Tuple[float, float]]:
    """
    使用Silero VAD检测连续语音片段
    
    参数:
        silence_threshold_sec: 静音阈值（秒），超过此时长视为片段分隔
        speech_pad_ms: 语音片段前后扩展的毫秒数，避免截断
    
    返回:
        连续语音片段列表 [(开始ms, 结束ms), ...]
    """
    print(f"正在检测连续语音片段（静音阈值: {silence_threshold_sec}秒）...")
    
    # 加载Silero VAD模型
    print("正在加载Silero VAD模型...")
    model, utils = torch.hub.load(
        repo_or_dir='snakers4/silero-vad',
        model='silero_vad',
        force_reload=False,
        onnx=False,
        trust_repo=True
    )
    print("模型加载完成")
    
    (get_speech_timestamps, save_audio, read_audio, VADIterator, collect_chunks) = utils
    
    # 读取音频
    print(f"正在读取音频文件...")
    wav = read_audio(audio_file, sampling_rate=16000)
    audio_length = len(wav) / 16000  # 总时长（秒）
    print(f"音频读取完成，总长度: {audio_length:.2f}秒")
    
    # 获取语音时间戳（提高阈值以减少背景音检测）
    print("正在分析语音活动...")
    speech_timestamps = get_speech_timestamps(
        wav, 
        model, 
        sampling_rate=16000,
        threshold=0.6,  # 提高阈值，减少背景音误检测（默认0.5）
        min_speech_duration_ms=250,  # 最小语音长度
        min_silence_duration_ms=100,  # 最小静音长度
        speech_pad_ms=speech_pad_ms   # 语音前后padding
    )
    
    if not speech_timestamps:
        print("未检测到任何语音")
        return []
    
    print(f"检测到 {len(speech_timestamps)} 个语音活动点")
    
    # 合并相近的语音片段为连续片段
    silence_threshold_ms = silence_threshold_sec * 1000
    continuous_segments = []
    
    current_start = speech_timestamps[0]['start'] / 16  # 转换为ms
    current_end = speech_timestamps[0]['end'] / 16
    
    for i in range(1, len(speech_timestamps)):
        next_start = speech_timestamps[i]['start'] / 16
        next_end = speech_timestamps[i]['end'] / 16
        
        # 如果间隔小于阈值，合并到当前片段
        if (next_start - current_end) < silence_threshold_ms:
            current_end = next_end
        else:
            # 保存当前片段，开始新片段
            continuous_segments.append((current_start, current_end))
            print(f"  连续片段: {current_start/1000:.2f}s - {current_end/1000:.2f}s "
                  f"(时长: {(current_end-current_start)/1000:.2f}s)")
            current_start = next_start
            current_end = next_end
    
    # 添加最后一个片段
    continuous_segments.append((current_start, current_end))
    print(f"  连续片段: {current_start/1000:.2f}s - {current_end/1000:.2f}s "
          f"(时长: {(current_end-current_start)/1000:.2f}s)")
    
    print(f"\n合并后共 {len(continuous_segments)} 个连续语音片段")
    return continuous_segments


def cut_audio_segment(audio_file: str, start_ms: float, end_ms: float, 
                     output_file: str) -> None:
    """切割单个音频片段"""
    start_sec = start_ms / 1000
    duration_sec = (end_ms - start_ms) / 1000
    
    cmd = [
        'ffmpeg', '-i', audio_file,
        '-ss', str(start_sec),
        '-t', str(duration_sec),
        '-ar', '16000', '-ac', '1',
        '-y', output_file
    ]
    
    subprocess.run(cmd, check=True, capture_output=True)


def transcribe_with_whisper(audio_file: str, language: str, model: str, 
                           output_dir: str) -> Dict:
    """使用whisper命令行工具转录音频"""
    print(f"    使用Whisper识别: {os.path.basename(audio_file)}")
    
    cmd = [
        'whisper', audio_file,
        '--model', model,
        '--language', language,
        '--output_format', 'json',
        '--output_dir', output_dir
    ]
    
    try:
        subprocess.run(cmd, check=True, timeout=120, capture_output=True)
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
        print(f"    ⚠️ Whisper识别失败: {e}")
        return {'segments': []}
    
    # 读取生成的JSON文件
    audio_basename = os.path.basename(audio_file).replace('.wav', '')
    json_file = os.path.join(output_dir, f"{audio_basename}.json")
    
    try:
        with open(json_file, 'r', encoding='utf-8') as f:
            result = json.load(f)
        
        # 重新保存JSON，使用真实文本而非unicode
        with open(json_file, 'w', encoding='utf-8') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        
        segment_count = len(result.get('segments', []))
        print(f"    识别完成: {segment_count} 个字幕片段")
        
        return result
    except FileNotFoundError:
        print(f"    ⚠️ 未找到JSON文件")
        return {'segments': []}
    
    finally:
        time.sleep(2)  # 等待2秒避免问题


def milliseconds_to_srt_time(ms: float) -> str:
    """将毫秒转换为SRT时间格式 HH:MM:SS,mmm"""
    hours = int(ms // 3600000)
    ms %= 3600000
    minutes = int(ms // 60000)
    ms %= 60000
    seconds = int(ms // 1000)
    milliseconds = int(ms % 1000)
    
    return f"{hours:02d}:{minutes:02d}:{seconds:02d},{milliseconds:03d}"


def generate_srt(all_segments: List[Dict], output_file: str) -> None:
    """生成SRT字幕文件"""
    print("\n正在生成SRT字幕文件...")
    
    subtitle_index = 1
    with open(output_file, 'w', encoding='utf-8') as f:
        for seg_info in all_segments:
            segment_start_ms = seg_info['segment_start_ms']
            whisper_result = seg_info['whisper_result']
            
            # 处理whisper识别的每个片段
            for segment in whisper_result.get('segments', []):
                # whisper的时间戳是相对于当前片段的秒数
                # 需要加上当前片段在原始音频中的起始位置
                start_ms = segment_start_ms + (segment['start'] * 1000)
                end_ms = segment_start_ms + (segment['end'] * 1000)
                text = segment['text'].strip()
                
                if text:  # 只有非空文本才写入
                    f.write(f"{subtitle_index}\n")
                    f.write(f"{milliseconds_to_srt_time(start_ms)} --> "
                           f"{milliseconds_to_srt_time(end_ms)}\n")
                    f.write(f"{text}\n")
                    f.write("\n")
                    subtitle_index += 1
    
    print(f"SRT字幕已生成: {output_file}")
    print(f"共 {subtitle_index - 1} 条字幕")


def main():
    parser = argparse.ArgumentParser(
        description='连续语音识别（改进版） - 针对对白稀疏的音频'
    )
    parser.add_argument('input_file', help='输入视频或音频文件')
    parser.add_argument('--language', required=True, 
                       help='语言名称（如：Japanese, English, Chinese）')
    parser.add_argument('--model', default='base', 
                       help='Whisper模型（默认：base）')
    parser.add_argument('--silence-threshold', type=float, default=2.0, 
                       help='静音阈值（秒），超过此时长视为片段分隔（默认：2.0）')
    parser.add_argument('--speech-pad', type=int, default=300,
                       help='语音片段前后padding（毫秒），避免截断（默认：300）')
    
    args = parser.parse_args()
    
    if not os.path.exists(args.input_file):
        print(f"错误：文件不存在 - {args.input_file}")
        return
    
    # 在当前目录创建temp_continuous文件夹
    temp_dir = os.path.join(os.getcwd(), 'temp_continuous')
    os.makedirs(temp_dir, exist_ok=True)
    print(f"临时目录: {temp_dir}")
    print("注意：处理完成后temp_continuous文件夹不会自动删除，请手动清理\n")
    
    # 步骤1: 提取音频
    audio_file = os.path.join(temp_dir, 'extracted_audio.wav')
    extract_audio(args.input_file, audio_file)
    print()
    
    # 步骤2: 检测连续语音片段
    continuous_segments = detect_continuous_speech_segments(
        audio_file, 
        args.silence_threshold,
        args.speech_pad
    )
    print()
    
    if not continuous_segments:
        print("未检测到语音片段")
        return
    
    # 步骤3 & 4: 切割并识别每个连续片段
    print("开始处理各个连续语音片段...\n")
    all_segments = []
    
    for i, (start_ms, end_ms) in enumerate(continuous_segments):
        print(f"【片段 {i+1}/{len(continuous_segments)}】")
        print(f"  位置: {start_ms/1000:.2f}s - {end_ms/1000:.2f}s")
        print(f"  时长: {(end_ms - start_ms)/1000:.2f}s")
        
        # 切割音频片段
        segment_file = os.path.join(temp_dir, f"segment_{i:04d}.wav")
        cut_audio_segment(audio_file, start_ms, end_ms, segment_file)
        
        # 用Whisper识别
        whisper_result = transcribe_with_whisper(
            segment_file,
            args.language,
            args.model,
            temp_dir
        )
        
        # 保存结果
        all_segments.append({
            'segment_index': i,
            'segment_start_ms': start_ms,
            'segment_end_ms': end_ms,
            'whisper_result': whisper_result
        })
        
        print()
    
    # 步骤5: 生成SRT文件
    output_srt = Path(args.input_file).stem + '.srt'
    generate_srt(all_segments, output_srt)
    
    # 输出统计信息
    total_speech_duration = sum(
        seg['segment_end_ms'] - seg['segment_start_ms'] 
        for seg in all_segments
    ) / 1000
    
    # 读取原始音频总时长
    import wave
    with wave.open(audio_file, 'rb') as wf:
        frames = wf.getnframes()
        rate = wf.getframerate()
        total_duration = frames / float(rate)
    
    print("\n" + "="*50)
    print("处理完成！")
    print("="*50)
    print(f"原始音频时长: {total_duration:.2f}秒")
    print(f"语音片段总时长: {total_speech_duration:.2f}秒")
    print(f"语音占比: {total_speech_duration/total_duration*100:.1f}%")
    print(f"连续片段数量: {len(continuous_segments)}")
    print(f"SRT文件: {output_srt}")
    print(f"临时文件: {temp_dir}")


if __name__ == '__main__':
    main()
