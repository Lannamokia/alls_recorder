import { useState, useEffect, useCallback } from 'react';
import axios from 'axios';
import { X } from 'lucide-react';
import BitrateHelper from './BitrateHelper';

interface UserSettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface UserConfig {
  max_bitrate: number;
  max_fps: number;
  resolution: string;
  monitor_id: string;
  desktop_audio: string;
  mic_audio: string;
  rtmp_url: string;
  rtmp_key: string;
  capture_mode: string;
  capture_method: string;
  window_id: string;
}

interface HardwareDevice {
  id: string;
  name: string;
}

interface HardwareInfo {
  screens: HardwareDevice[];
  desktop_audio: HardwareDevice[];
  microphone: HardwareDevice[];
  encoders: HardwareDevice[];
  windows?: { title: string; exe: string; id: string }[];
}

const deviceForbiddenChars = ['&', '|', ';', '$', '`', '>', '<', '(', ')', '[', ']', '\\', '"', '\'', '\n', '\r'];
const rtmpUrlForbiddenChars = ['&', '|', ';', '$', '`', '>', '<', '(', ')', '{', '}', '[', ']', '\\', '"', '\'', '\n', '\r'];
const resolutionLabelSet = new Set(['4k', '2160p', '1080p', '720p', '480p']);

const validateNonNegativeNumber = (value: number, label: string) => {
  if (Number.isNaN(value)) return `${label}不合法`;
  if (value < 0) return `${label}不能为负数`;
  return '';
};

const validateResolutionValue = (value: string) => {
  const v = value.trim().toLowerCase();
  if (!v) return '分辨率不能为空';
  if (resolutionLabelSet.has(v)) return '';
  if (/^\d+x\d+$/.test(v)) return '';
  return '分辨率格式不正确';
};

const validateDeviceIdValue = (value: string, label: string) => {
  if (!value) return '';
  if ([...value].some(c => deviceForbiddenChars.includes(c))) return `${label}包含非法字符`;
  if (value.includes('..')) return `${label}不允许包含..`;
  return '';
};

const validateWindowIdValue = (value: string) => {
  if (!value) return '';
  if ([...value].some(c => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127)) return '录制窗口包含非法字符';
  return '';
};

const validateRtmpUrlValue = (value: string) => {
  if (!value) return '';
  if (!value.startsWith('rtmp://') && !value.startsWith('rtmps://')) return 'RTMP 地址格式错误';
  if ([...value].some(c => rtmpUrlForbiddenChars.includes(c))) return 'RTMP 地址包含非法字符';
  return '';
};

const validateRtmpKeyValue = (value: string) => {
  if (!value) return '';
  if ([...value].some(c => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127)) return 'RTMP Key 包含非法控制字符';
  return '';
};

const resolutionOptions = [
  { value: '3840x2160', label: '4K 横屏 (3840x2160)', rank: 3 },
  { value: '2160x3840', label: '4K 竖屏 (2160x3840)', rank: 3 },
  { value: '1920x1080', label: '1080p 横屏 (1920x1080)', rank: 2 },
  { value: '1080x1920', label: '1080p 竖屏 (1080x1920)', rank: 2 },
  { value: '1280x720', label: '720p 横屏 (1280x720)', rank: 1 },
  { value: '720x1280', label: '720p 竖屏 (720x1280)', rank: 1 },
  { value: '854x480', label: '480p 横屏 (854x480)', rank: 0 },
  { value: '480x854', label: '480p 竖屏 (480x854)', rank: 0 }
];

const rankFromValue = (value: string) => {
  const v = value.trim().toLowerCase();
  if (v === '4k' || v === '2160p') return 3;
  if (v === '1080p') return 2;
  if (v === '720p') return 1;
  if (v === '480p') return 0;
  const parts = v.split('x');
  if (parts.length === 2) {
    const w = parseInt(parts[0], 10);
    const h = parseInt(parts[1], 10);
    const maxSide = Math.max(w || 0, h || 0);
    if (maxSide >= 3000) return 3;
    if (maxSide >= 1900) return 2;
    if (maxSide >= 1200) return 1;
    return 0;
  }
  return 2;
};

export default function UserSettingsModal({ isOpen, onClose }: UserSettingsModalProps) {
  const [config, setConfig] = useState<UserConfig>({
    max_bitrate: 4000,
    max_fps: 30,
    resolution: '1920x1080',
    monitor_id: '',
    desktop_audio: '',
    mic_audio: '',
    rtmp_url: '',
    rtmp_key: '',
    capture_mode: 'screen',
    capture_method: 'auto',
    window_id: ''
  });
  const [hardwareInfo, setHardwareInfo] = useState<HardwareInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState('');
  const [msgType, setMsgType] = useState<'success' | 'error' | ''>('');
  const [maxResRank, setMaxResRank] = useState(2);
  const [systemRecordConfig, setSystemRecordConfig] = useState<{ max_bitrate: number; max_fps: number; max_res: string } | null>(null);

  const token = localStorage.getItem('token');
  const baseUrl = localStorage.getItem('backend_url') || 'http://localhost:3000';

  const fetchConfig = useCallback(async () => {
    setLoading(true);
    try {
      const headers = token ? { Authorization: `Bearer ${token}` } : undefined;
      const [userRes, recordRes, hardwareRes] = await Promise.all([
        axios.get(`${baseUrl}/api/user/config`, { headers }),
        axios.get(`${baseUrl}/api/settings/record-config`, { headers }).catch(() => null),
        axios.get(`${baseUrl}/api/hardware/info`, { headers }).catch(() => null)
      ]);
      const maxResValue = recordRes?.data?.max_res || '1080p';
      if (recordRes?.data) {
        setSystemRecordConfig(recordRes.data);
      } else {
        setSystemRecordConfig(null);
      }
      const rank = rankFromValue(maxResValue);
      setMaxResRank(rank);
      const allowedValues = resolutionOptions.filter(o => o.rank <= rank).map(o => o.value);
      const resData = userRes.data;
      const nextResolution = allowedValues.includes(resData.resolution) ? resData.resolution : allowedValues[0] || '1920x1080';
      // Merge with defaults if nulls
      setConfig(prev => ({
        ...prev,
        ...resData,
        max_bitrate: resData.max_bitrate || 4000,
        max_fps: resData.max_fps || 30,
        resolution: nextResolution,
        monitor_id: resData.monitor_id || '',
        desktop_audio: resData.desktop_audio || '',
        mic_audio: resData.mic_audio || '',
        rtmp_url: resData.rtmp_url || '',
        rtmp_key: resData.rtmp_key || '',
        capture_mode: resData.capture_mode || 'screen',
        capture_method: resData.capture_method || 'auto',
        window_id: resData.window_id || ''
      }));
      setHardwareInfo(hardwareRes?.data || null);
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  }, [baseUrl, token]);

  useEffect(() => {
    if (isOpen) {
      fetchConfig();
    }
  }, [fetchConfig, isOpen]);

  const handleSave = async () => {
    setSaving(true);
    setMsg('');
    setMsgType('');
    const errors: string[] = [];
    const resolutionError = validateResolutionValue(config.resolution);
    if (resolutionError) errors.push(resolutionError);
    const fpsError = validateNonNegativeNumber(config.max_fps, '帧率');
    if (fpsError) errors.push(fpsError);
    const bitrateError = validateNonNegativeNumber(config.max_bitrate, '码率');
    if (bitrateError) errors.push(bitrateError);
    const monitorError = validateDeviceIdValue(config.monitor_id, '录制屏幕');
    if (monitorError) errors.push(monitorError);
    const windowError = validateWindowIdValue(config.window_id);
    if (windowError) errors.push(windowError);
    const desktopAudioError = validateDeviceIdValue(config.desktop_audio, '桌面音频');
    if (desktopAudioError) errors.push(desktopAudioError);
    const micAudioError = validateDeviceIdValue(config.mic_audio, '麦克风');
    if (micAudioError) errors.push(micAudioError);
    const rtmpUrlError = validateRtmpUrlValue(config.rtmp_url);
    if (rtmpUrlError) errors.push(rtmpUrlError);
    const rtmpKeyError = validateRtmpKeyValue(config.rtmp_key);
    if (rtmpKeyError) errors.push(rtmpKeyError);
    if (systemRecordConfig) {
      if (config.max_fps > systemRecordConfig.max_fps) {
        errors.push(`帧率超过系统限制 ${systemRecordConfig.max_fps}`);
      }
      if (config.max_bitrate > systemRecordConfig.max_bitrate) {
        errors.push(`码率超过系统限制 ${systemRecordConfig.max_bitrate}`);
      }
    }
    if (config.capture_mode === 'window' && !config.window_id) {
      errors.push('请选择录制窗口');
    }
    if (errors.length > 0) {
      setMsg(`保存失败：${errors[0]}`);
      setMsgType('error');
      setSaving(false);
      return;
    }
    try {
      await axios.post(`${baseUrl}/api/user/config`, config, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setMsg('设置已保存');
      setMsgType('success');
      setTimeout(() => {
          setMsg('');
          setMsgType('');
          onClose();
      }, 1000);
    } catch (err) {
      console.error(err);
      if (axios.isAxiosError(err)) {
        const data = err.response?.data;
        const reason = typeof data === 'string' && data ? data : '未知原因';
        setMsg(`保存失败：${reason}`);
        setMsgType('error');
      } else {
        setMsg('保存失败：未知原因');
        setMsgType('error');
      }
    } finally {
      setSaving(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-lg w-full max-w-lg p-6 relative">
        <button onClick={onClose} className="absolute top-4 right-4 text-gray-500 hover:text-gray-700">
          <X size={24} />
        </button>
        
        <h2 className="text-xl font-bold mb-4">录制设置</h2>
        
        {msg && <div className={`mb-4 p-2 rounded ${msgType === 'error' ? 'bg-red-100 text-red-700' : 'bg-green-100 text-green-700'}`}>{msg}</div>}

        {loading ? (
            <p>加载中...</p>
        ) : (
            <div className="space-y-4 max-h-[70vh] overflow-y-auto">
                <div>
                    <label className="block text-sm font-medium mb-1">分辨率</label>
                    <select 
                        value={config.resolution}
                        onChange={e => setConfig({...config, resolution: e.target.value})}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                    >
                        {resolutionOptions.filter(o => o.rank <= maxResRank).map(o => (
                          <option key={o.value} value={o.value}>{o.label}</option>
                        ))}
                    </select>
                </div>

                <div>
                    <label className="block text-sm font-medium mb-1">采集模式</label>
                    <select
                        value={config.capture_mode}
                        onChange={e => setConfig({...config, capture_mode: e.target.value})}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                    >
                        <option value="screen">屏幕采集</option>
                        <option value="window">窗口采集</option>
                    </select>
                </div>

                {config.capture_mode === 'screen' && (
                  <div>
                      <label className="block text-sm font-medium mb-1">录制屏幕</label>
                      <select
                          value={config.monitor_id}
                          onChange={e => setConfig({...config, monitor_id: e.target.value})}
                          className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                          disabled={!hardwareInfo?.screens?.length}
                      >
                          <option value="">默认/自动</option>
                          {config.monitor_id && !hardwareInfo?.screens?.some(s => s.id === config.monitor_id) && (
                              <option value={config.monitor_id}>{config.monitor_id}</option>
                          )}
                          {hardwareInfo?.screens?.map(s => (
                              <option key={s.id} value={s.id}>{s.name}</option>
                          ))}
                      </select>
                  </div>
                )}

                {config.capture_mode === 'screen' && (
                  <div>
                      <label className="block text-sm font-medium mb-1">采集路径</label>
                      <select
                          value={config.capture_method}
                          onChange={e => setConfig({...config, capture_method: e.target.value})}
                          className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                      >
                          <option value="auto">自动</option>
                          <option value="dxgi">DXGI</option>
                          <option value="wgc">WGC</option>
                      </select>
                      <p className="text-xs text-red-500 mt-1">Windows 10 1809 版本以下无法使用 WGC 采集</p>
                  </div>
                )}

                {config.capture_mode === 'window' && (
                  <div>
                      <label className="block text-sm font-medium mb-1">录制窗口</label>
                      <select
                          value={config.window_id}
                          onChange={e => setConfig({...config, window_id: e.target.value})}
                          className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                          disabled={!hardwareInfo?.windows?.length}
                      >
                          <option value="">请选择窗口</option>
                          {config.window_id && !hardwareInfo?.windows?.some(w => w.id === config.window_id) && (
                              <option value={config.window_id}>{config.window_id}</option>
                          )}
                          {hardwareInfo?.windows?.map(w => (
                              <option key={w.id} value={w.id}>{`${w.title} (${w.exe})`}</option>
                          ))}
                      </select>
                  </div>
                )}

                <div className="grid grid-cols-2 gap-4">
                    <div>
                        <label className="block text-sm font-medium mb-1 h-6 flex items-center">帧率 (FPS)</label>
                        <input 
                            type="number" 
                            value={config.max_fps}
                            onChange={e => setConfig({...config, max_fps: parseInt(e.target.value)})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        />
                    </div>
                    <div>
                        <label className="block text-sm font-medium mb-1 h-6 flex items-center">
                            码率 (Kbps)
                            <BitrateHelper />
                        </label>
                        <input 
                            type="number" 
                            value={config.max_bitrate}
                            onChange={e => setConfig({...config, max_bitrate: parseInt(e.target.value)})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        />
                    </div>
                </div>

                <div>
                    <label className="block text-sm font-medium mb-1">桌面音频设备</label>
                    <select
                        value={config.desktop_audio}
                        onChange={e => setConfig({...config, desktop_audio: e.target.value})}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        disabled={!hardwareInfo?.desktop_audio?.length}
                    >
                        <option value="">关闭</option>
                        <option value="default">系统默认</option>
                        {config.desktop_audio && !hardwareInfo?.desktop_audio?.some(d => d.id === config.desktop_audio) && (
                            <option value={config.desktop_audio}>{config.desktop_audio}</option>
                        )}
                        {hardwareInfo?.desktop_audio?.map(d => (
                            <option key={d.id} value={d.id}>{d.name}</option>
                        ))}
                    </select>
                </div>

                <div>
                    <label className="block text-sm font-medium mb-1">麦克风设备</label>
                    <select
                        value={config.mic_audio}
                        onChange={e => setConfig({...config, mic_audio: e.target.value})}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        disabled={!hardwareInfo?.microphone?.length}
                    >
                        <option value="">关闭</option>
                        <option value="default">系统默认</option>
                        {config.mic_audio && !hardwareInfo?.microphone?.some(d => d.id === config.mic_audio) && (
                            <option value={config.mic_audio}>{config.mic_audio}</option>
                        )}
                        {hardwareInfo?.microphone?.map(d => (
                            <option key={d.id} value={d.id}>{d.name}</option>
                        ))}
                    </select>
                </div>

                <div className="border-t pt-4 mt-4">
                    <h3 className="text-md font-semibold mb-2">推流设置 (RTMP)</h3>
                    <div className="mb-2">
                        <label className="block text-sm font-medium mb-1">推流地址</label>
                        <input 
                            type="text" 
                            value={config.rtmp_url}
                            onChange={e => setConfig({...config, rtmp_url: e.target.value})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                            placeholder="rtmp://服务器地址/直播间"
                        />
                    </div>
                    <div>
                        <label className="block text-sm font-medium mb-1">推流密钥</label>
                        <input 
                            type="password" 
                            value={config.rtmp_key}
                            onChange={e => setConfig({...config, rtmp_key: e.target.value})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        />
                    </div>
                </div>
            </div>
        )}

        <div className="mt-6 flex justify-end space-x-3">
            <button onClick={onClose} className="px-4 py-2 border rounded hover:bg-gray-100 dark:hover:bg-gray-700">取消</button>
            <button 
                onClick={handleSave} 
                disabled={saving}
                className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded font-bold"
            >
                {saving ? '保存中...' : '保存'}
            </button>
        </div>
      </div>
    </div>
  );
}
