# cli-capture-standalone

基于 OBS Studio 的 libobs 抽出并重写调用程序的独立版命令行采集工具。

## 构建

Windows：

```bash
scripts\build_windows.bat
```

产物由构建脚本输出到默认目录。

## CLI 参数

用法：

```powershell
cli-capture.exe [options]
```

参数列表：

- `--scan`  
  输出本机硬件探测结果（屏幕、音频设备、编码器）为 JSON，并退出。
- `--monitor <index>`  
  选择要采集的屏幕索引，从 0 开始。
- `--desktop-audio <device_id>`  
  指定桌面音频设备 ID。
- `--mic-audio <device_id>`  
  指定麦克风设备 ID。
- `--output <file>`  
  录制输出文件路径。
- `--rtmp <url>`  
  推流服务器地址。
- `--key <key>`  
  推流密钥。
- `--encoder <encoder_id>`  
  指定视频编码器 ID。
- `--bitrate <kbps>`  
  视频码率，单位 kbps。
- `--width <px>`  
  输出宽度，单位像素。
- `--height <px>`  
  输出高度，单位像素。
- `--fps <value>`  
  帧率。

示例：

```powershell
cli-capture.exe --scan

>>{
  "screens": [
    { "id": "0", "name": "Display 1: 2560x1440 @ 0,0 (Primary Monitor)" }
  ],
  "desktop_audio": [
    { "id": "default", "name": "Default" },
    { "id": "{0.0.0.00000000}.{706019c6-8bc4-479c-8595-136e13a9953b}", "name": "扬声器 (Realtek(R) Audio)" }
  ],
  "microphone": [
    { "id": "default", "name": "Default" },
    { "id": "{0.0.1.00000000}.{d79c3f7c-06f3-403f-ba64-ae723d3848e2}", "name": "麦克风阵列 (Realtek(R) Audio)" }
  ],
  "encoders": [
    { "id": "obs_nvenc_h264_tex", "name": "NVIDIA NVENC H.264" },
    { "id": "obs_nvenc_h264_soft", "name": "NVIDIA NVENC H.264 (Fallback)" },
    { "id": "obs_nvenc_hevc_tex", "name": "NVIDIA NVENC HEVC" },
    { "id": "obs_nvenc_hevc_soft", "name": "NVIDIA NVENC HEVC (Fallback)" },
    { "id": "obs_nvenc_av1_tex", "name": "NVIDIA NVENC AV1" },
    { "id": "obs_nvenc_av1_soft", "name": "NVIDIA NVENC AV1 (Fallback)" },
    { "id": "jim_nvenc", "name": "NVIDIA NVENC H.264" },
    { "id": "obs_nvenc_h264_cuda", "name": "NVIDIA NVENC H.264" },
    { "id": "jim_hevc_nvenc", "name": "NVIDIA NVENC HEVC" },
    { "id": "obs_nvenc_hevc_cuda", "name": "NVIDIA NVENC HEVC" },
    { "id": "jim_av1_nvenc", "name": "NVIDIA NVENC AV1" },
    { "id": "obs_nvenc_av1_cuda", "name": "NVIDIA NVENC AV1" },
    { "id": "ffmpeg_nvenc", "name": "NVIDIA NVENC H.264" },
    { "id": "ffmpeg_hevc_nvenc", "name": "NVIDIA NVENC HEVC" },
    { "id": "obs_x264", "name": "x264" }
  ]

}
```

```powershell
cli-capture.exe --monitor 0 --width 1920 --height 1080 --fps 60 --encoder "<encoder_id>" --bitrate 2500 --desktop-audio "<desktop_audio_device_id>" --mic-audio "<mic_device_id>" --output "C:\captures\session_2026-02-16.mp4"
```

```powershell
cli-capture.exe --monitor 1 --width 1280 --height 720 --fps 30 --encoder "<encoder_id>" --bitrate 1500 --desktop-audio "<desktop_audio_device_id>" --mic-audio "<mic_device_id>" --rtmp "rtmp://live.example.com/app" --key "live_abc123"
```

## 许可证与第三方依赖

本项目使用并链接 OBS Studio 的 libobs，发布时需遵守 GPL v2 及第三方依赖许可证。

- OBS GPL v2：LICENSES\OBS-GPLv2.txt
- 源码树内第三方许可证：LICENSES\source-tree\
- 预编译依赖许可证：LICENSES\prebuilt\

