# CLI 参数说明

## 用法

```powershell
cli-capture.exe [options]
```

## 参数列表

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

## 示例

```powershell
cli-capture.exe --monitor 0 --width 1920 --height 1080 --fps 60 --encoder obs_nvenc_hevc --bitrate 2500 --desktop-audio "<desktop_audio_device_id>" --mic-audio "<mic_device_id>" --output "C:\temp\test.mp4"
```
