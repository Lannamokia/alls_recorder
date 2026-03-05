# CLI 参数说明

## 用法

```powershell
cli-capture.exe [options]
```

## 参数列表

### 扫描模式

- `--scan`
  输出本机硬件探测结果（屏幕、音频设备、编码器）为 JSON，并退出。

- `--scan-windows`
  输出当前可采集的窗口列表为 JSON，并退出。

### 采集目标

- `--monitor <index 或 device_id>`
  选择要采集的屏幕。支持整数索引（从 0 开始）或 `--scan` 输出的设备接口路径字符串。

- `--window <window_id>`
  窗口采集模式，指定窗口 ID（格式：`标题:进程名:窗口类名`，可从 `--scan-windows` 获取）。

- `--method <auto|dxgi|wgc>`
  屏幕采集方法。默认 auto。

### 输出

- `--output <file>`
  录制输出文件路径。

- `--rtmp <url>`
  推流服务器地址。

- `--key <key>`
  推流密钥。

### 编码

- `--encoder <encoder_id>`
  指定视频编码器 ID（如 `obs_nvenc_h264_tex`、`obs_x264`）。

- `--bitrate <kbps>`
  视频码率，单位 kbps。

### 画面

- `--width <px>`
  输出编码宽度。不指定则使用显示器原生分辨率。

- `--height <px>`
  输出编码高度。不指定则使用显示器原生分辨率。

- `--fps <value>`
  帧率，默认 30。

### 音频

- `--desktop-audio <device_id>`
  指定桌面音频设备 ID。

- `--mic-audio <device_id>`
  指定麦克风设备 ID。

### 其他

- `--test`
  测试模式，创建 source 后立即退出，不启动输出。

## 示例

屏幕录制（自动检测分辨率，编码输出 1080p）：

```powershell
cli-capture.exe --monitor 0 --width 1920 --height 1080 --fps 60 --encoder obs_nvenc_h264_tex --bitrate 2500 --desktop-audio "{0.0.0.00000000}.{...}" --output test.mp4
```

窗口录制：

```powershell
cli-capture.exe --window "记事本:notepad.exe:Notepad" --width 1920 --height 1080 --fps 60 --encoder obs_x264 --bitrate 2500 --output test.mp4
```

推流：

```powershell
cli-capture.exe --monitor 0 --fps 30 --encoder obs_nvenc_h264_tex --bitrate 4000 --rtmp "rtmp://live.example.com/live" --key "stream_key"
```
