import { useState, useEffect } from 'react';
import axios from 'axios';
import { X } from 'lucide-react';

interface UserSettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface UserConfig {
  max_bitrate: number;
  max_fps: number;
  resolution: string;
  desktop_audio: string;
  mic_audio: string;
  rtmp_url: string;
  rtmp_key: string;
}

export default function UserSettingsModal({ isOpen, onClose }: UserSettingsModalProps) {
  const [config, setConfig] = useState<UserConfig>({
    max_bitrate: 4000,
    max_fps: 30,
    resolution: '1920x1080',
    desktop_audio: '',
    mic_audio: '',
    rtmp_url: '',
    rtmp_key: ''
  });
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState('');
  const [msgType, setMsgType] = useState<'success' | 'error' | ''>('');
  const [maxResRank, setMaxResRank] = useState(2);

  const token = localStorage.getItem('token');
  const baseUrl = localStorage.getItem('backend_url') || 'http://localhost:3000';

  useEffect(() => {
    if (isOpen) {
      fetchConfig();
    }
  }, [isOpen]);

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

  const fetchConfig = async () => {
    setLoading(true);
    try {
      const headers = token ? { Authorization: `Bearer ${token}` } : undefined;
      const [userRes, recordRes] = await Promise.all([
        axios.get(`${baseUrl}/api/user/config`, { headers }),
        axios.get(`${baseUrl}/api/settings/record-config`, { headers }).catch(() => null)
      ]);
      const maxResValue = recordRes?.data?.max_res || '1080p';
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
        desktop_audio: resData.desktop_audio || '',
        mic_audio: resData.mic_audio || '',
        rtmp_url: resData.rtmp_url || '',
        rtmp_key: resData.rtmp_key || ''
      }));
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    setMsg('');
    setMsgType('');
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

                <div className="grid grid-cols-2 gap-4">
                    <div>
                        <label className="block text-sm font-medium mb-1">帧率 (FPS)</label>
                        <input 
                            type="number" 
                            value={config.max_fps}
                            onChange={e => setConfig({...config, max_fps: parseInt(e.target.value)})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        />
                    </div>
                    <div>
                        <label className="block text-sm font-medium mb-1">码率 (Kbps)</label>
                        <input 
                            type="number" 
                            value={config.max_bitrate}
                            onChange={e => setConfig({...config, max_bitrate: parseInt(e.target.value)})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        />
                    </div>
                </div>

                <div>
                    <label className="block text-sm font-medium mb-1">桌面音频设备 ID (可选)</label>
                    <input 
                        type="text" 
                        value={config.desktop_audio}
                        onChange={e => setConfig({...config, desktop_audio: e.target.value})}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        placeholder="例如: default 或设备GUID"
                    />
                </div>

                <div>
                    <label className="block text-sm font-medium mb-1">麦克风设备 ID (可选)</label>
                    <input 
                        type="text" 
                        value={config.mic_audio}
                        onChange={e => setConfig({...config, mic_audio: e.target.value})}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        placeholder="例如: default 或设备GUID"
                    />
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
                            placeholder="rtmp://..."
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
