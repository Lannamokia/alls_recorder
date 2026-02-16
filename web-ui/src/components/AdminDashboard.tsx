import { useState, useEffect } from 'react';
import axios from 'axios';
import { Trash2, RefreshCw, Key } from 'lucide-react';

export default function AdminDashboard() {
  const [activeTab, setActiveTab] = useState<'users' | 'settings' | 'actions'>('actions');
  const [error, setError] = useState('');
  const [success, setSuccess] = useState('');

  const token = localStorage.getItem('token');
  const baseUrl = localStorage.getItem('backend_url') || 'http://localhost:3000';

  return (
    <div className="bg-white dark:bg-gray-800 p-6 rounded-lg shadow-md">
      <h2 className="text-xl font-semibold mb-6">管理员控制台</h2>
      
      {error && <div className="bg-red-100 text-red-700 p-3 rounded mb-4">{error}</div>}
      {success && <div className="bg-green-100 text-green-700 p-3 rounded mb-4">{success}</div>}

      <div className="flex border-b mb-6 dark:border-gray-700">
        <button 
          className={`px-4 py-2 font-medium ${activeTab === 'actions' ? 'text-blue-600 border-b-2 border-blue-600' : 'text-gray-500 hover:text-gray-700'}`}
          onClick={() => setActiveTab('actions')}
        >
          常规操作
        </button>
        <button 
          className={`px-4 py-2 font-medium ${activeTab === 'users' ? 'text-blue-600 border-b-2 border-blue-600' : 'text-gray-500 hover:text-gray-700'}`}
          onClick={() => setActiveTab('users')}
        >
          用户管理
        </button>
        <button 
          className={`px-4 py-2 font-medium ${activeTab === 'settings' ? 'text-blue-600 border-b-2 border-blue-600' : 'text-gray-500 hover:text-gray-700'}`}
          onClick={() => setActiveTab('settings')}
        >
          系统设置
        </button>
      </div>

      {activeTab === 'actions' && <AdminActions token={token} baseUrl={baseUrl} setError={setError} setSuccess={setSuccess} />}
      {activeTab === 'users' && <UserManagement token={token} baseUrl={baseUrl} setError={setError} setSuccess={setSuccess} />}
      {activeTab === 'settings' && <SystemSettings token={token} baseUrl={baseUrl} setError={setError} setSuccess={setSuccess} />}
    </div>
  );
}

function AdminActions({ token, baseUrl, setError, setSuccess }: any) {
  const [scanning, setScanning] = useState(false);
  const [scanResult, setScanResult] = useState<any>(null);
  const [announcement, setAnnouncement] = useState('');
  const [posting, setPosting] = useState(false);
  const [announcements, setAnnouncements] = useState<any[]>([]);

  const fetchAnnouncements = async () => {
    try {
      const res = await axios.get(`${baseUrl}/api/announcements`, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setAnnouncements(res.data);
    } catch (err) {
      console.error(err);
    }
  };

  useEffect(() => {
    fetchAnnouncements();
  }, []);

  const handleScan = async () => {
    setScanning(true);
    setError('');
    setScanResult(null);
    try {
      const res = await axios.post(`${baseUrl}/api/hardware/scan`, {}, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setScanResult(res.data);
      setSuccess('硬件扫描完成');
    } catch (err: any) {
      console.error(err);
      setError(err.response?.data || '扫描失败');
    } finally {
      setScanning(false);
    }
  };

  const handlePostAnnouncement = async () => {
    if (!announcement.trim()) return;
    setPosting(true);
    try {
      await axios.post(`${baseUrl}/api/announcements`, { content: announcement }, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setSuccess('公告已发布');
      setAnnouncement('');
      fetchAnnouncements();
    } catch (err: any) {
      console.error(err);
      setError(err.response?.data || '发布失败');
    } finally {
      setPosting(false);
    }
  };

  const handleDeleteAnnouncement = async (id: string) => {
    if (!confirm('确定要删除此公告吗？')) return;
    try {
      await axios.delete(`${baseUrl}/api/announcements/${id}`, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setSuccess('公告已删除');
      fetchAnnouncements();
    } catch (err: any) {
      setError('删除失败');
    }
  };

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
      <div className="p-4 border rounded dark:border-gray-700">
        <h3 className="font-medium mb-2">硬件检测</h3>
        <button 
          onClick={handleScan} 
          disabled={scanning}
          className="bg-purple-600 hover:bg-purple-700 text-white font-bold py-2 px-4 rounded w-full"
        >
          {scanning ? '扫描中...' : '运行硬件检测'}
        </button>
        {scanResult && (
          <div className="mt-4 p-4 bg-gray-100 dark:bg-gray-700 rounded overflow-auto max-h-48">
            <pre className="text-xs">{JSON.stringify(scanResult, null, 2)}</pre>
          </div>
        )}
      </div>

      <div className="p-4 border rounded dark:border-gray-700">
        <h3 className="font-medium mb-2">发布公告</h3>
        <textarea
          value={announcement}
          onChange={e => setAnnouncement(e.target.value)}
          className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600 mb-2 h-24"
          placeholder="输入公告内容..."
        />
        <button 
          onClick={handlePostAnnouncement} 
          disabled={posting || !announcement.trim()}
          className="bg-blue-600 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded w-full"
        >
          {posting ? '发布中...' : '发布公告'}
        </button>

        <div className="mt-6">
            <h4 className="font-medium mb-2 text-sm text-gray-600 dark:text-gray-400">已发布公告</h4>
            <div className="space-y-2 max-h-60 overflow-y-auto">
                {announcements.map((a) => (
                    <div key={a.id} className="p-2 bg-gray-50 dark:bg-gray-800 rounded flex justify-between items-start text-sm">
                        <div className="flex-1 mr-2">
                            <p className="break-all">{a.content}</p>
                            <p className="text-xs text-gray-400 mt-1">{new Date(a.created_at).toLocaleString()}</p>
                        </div>
                        <button 
                            onClick={() => handleDeleteAnnouncement(a.id)}
                            className="text-red-500 hover:text-red-700 p-1"
                            title="删除公告"
                        >
                            <Trash2 size={14} />
                        </button>
                    </div>
                ))}
                {announcements.length === 0 && <p className="text-gray-400 text-xs text-center py-2">暂无公告</p>}
            </div>
        </div>
      </div>
    </div>
  );
}

function UserManagement({ token, baseUrl, setError, setSuccess }: any) {
    const [users, setUsers] = useState<any[]>([]);
    const [loading, setLoading] = useState(false);

    const fetchUsers = async () => {
        setLoading(true);
        try {
            const res = await axios.get(`${baseUrl}/api/users`, {
                headers: { Authorization: `Bearer ${token}` }
            });
            setUsers(res.data);
        } catch (err: any) {
            setError('获取用户列表失败');
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        fetchUsers();
    }, []);

    const handleDelete = async (id: string) => {
        if (!confirm('确定要删除此用户吗？此操作不可逆。')) return;
        try {
            await axios.delete(`${baseUrl}/api/users/${id}`, {
                headers: { Authorization: `Bearer ${token}` }
            });
            setSuccess('用户已删除');
            fetchUsers();
        } catch (err) {
            setError('删除失败');
        }
    };

    const handleResetPassword = async (id: string) => {
        const newPass = prompt('请输入新密码:');
        if (!newPass) return;
        try {
            await axios.post(`${baseUrl}/api/users/${id}/reset-password`, { new_password: newPass }, {
                headers: { Authorization: `Bearer ${token}` }
            });
            setSuccess('密码已重置');
        } catch (err) {
            setError('重置密码失败');
        }
    };

    return (
        <div>
            <div className="flex justify-between mb-4">
                <h3 className="font-medium">用户列表</h3>
                <button onClick={fetchUsers} className="text-blue-500 hover:underline flex items-center"><RefreshCw size={16} className="mr-1"/> 刷新</button>
            </div>
            {loading ? <p>加载中...</p> : (
                <div className="overflow-x-auto">
                    <table className="w-full text-left border-collapse">
                        <thead>
                            <tr className="border-b dark:border-gray-700">
                                <th className="p-2">用户名</th>
                                <th className="p-2">角色</th>
                                <th className="p-2">注册时间</th>
                                <th className="p-2">操作</th>
                            </tr>
                        </thead>
                        <tbody>
                            {users.map(user => (
                                <tr key={user.id} className="border-b dark:border-gray-700">
                                    <td className="p-2">{user.username}</td>
                                    <td className="p-2"><span className={`px-2 py-0.5 rounded text-xs ${user.role === 'admin' ? 'bg-purple-100 text-purple-800' : 'bg-gray-100 text-gray-800'}`}>{user.role}</span></td>
                                    <td className="p-2 text-sm text-gray-500">{new Date(user.created_at).toLocaleDateString()}</td>
                                    <td className="p-2 flex space-x-2">
                                        <button onClick={() => handleResetPassword(user.id)} className="text-orange-500 hover:text-orange-700" title="重置密码"><Key size={18}/></button>
                                        <button onClick={() => handleDelete(user.id)} className="text-red-500 hover:text-red-700" title="删除用户"><Trash2 size={18}/></button>
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            )}
        </div>
    );
}

function SystemSettings({ token, baseUrl, setError, setSuccess }: any) {
    const [cliPath, setCliPath] = useState('');
    const [globalPath, setGlobalPath] = useState('');
    const [downloadTokenTtlMinutes, setDownloadTokenTtlMinutes] = useState(60);
    const [serverName, setServerName] = useState('');
    const [recordConfig, setRecordConfig] = useState({
        max_bitrate: 4000,
        max_fps: 30,
        max_res: '1080p',
        video_encoder: 'x264'
    });
    const [loading, setLoading] = useState(false);
    const [saving, setSaving] = useState(false);

    useEffect(() => {
        fetchSettings();
    }, []);

    const normalizeMaxRes = (value: string) => {
        const v = value.trim().toLowerCase();
        if (v === '4k' || v === '2160p') return '4k';
        if (v === '1080p') return '1080p';
        if (v === '720p') return '720p';
        const parts = v.split('x');
        if (parts.length === 2) {
            const w = parseInt(parts[0], 10);
            const h = parseInt(parts[1], 10);
            const maxSide = Math.max(w || 0, h || 0);
            if (maxSide >= 3000) return '4k';
            if (maxSide >= 1900) return '1080p';
            if (maxSide >= 1200) return '720p';
            return '720p';
        }
        return '1080p';
    };

    const fetchSettings = async () => {
        setLoading(true);
        try {
            const [pathRes, configRes, globalPathRes, ttlRes, nameRes] = await Promise.all([
                axios.get(`${baseUrl}/api/settings/cli-path`, { headers: { Authorization: `Bearer ${token}` } }),
                axios.get(`${baseUrl}/api/settings/record-config`, { headers: { Authorization: `Bearer ${token}` } }),
                axios.get(`${baseUrl}/api/settings/global-path`, { headers: { Authorization: `Bearer ${token}` } }),
                axios.get(`${baseUrl}/api/settings/download-token-ttl`, { headers: { Authorization: `Bearer ${token}` } }),
                axios.get(`${baseUrl}/api/settings/server-name`, { headers: { Authorization: `Bearer ${token}` } })
            ]);
            setCliPath(pathRes.data.path);
            setRecordConfig({
                ...configRes.data,
                max_res: normalizeMaxRes(configRes.data.max_res || '1080p')
            });
            setGlobalPath(globalPathRes.data.path);
            setDownloadTokenTtlMinutes(ttlRes.data.minutes ?? 60);
            setServerName(nameRes.data.name ?? '');
        } catch (err) {
            console.error(err);
        } finally {
            setLoading(false);
        }
    };

    const handleSave = async () => {
        setSaving(true);
        try {
            await Promise.all([
                axios.post(`${baseUrl}/api/settings/cli-path`, { path: cliPath }, { headers: { Authorization: `Bearer ${token}` } }),
                axios.post(`${baseUrl}/api/settings/record-config`, recordConfig, { headers: { Authorization: `Bearer ${token}` } }),
                axios.post(`${baseUrl}/api/settings/global-path`, { path: globalPath }, { headers: { Authorization: `Bearer ${token}` } }),
                axios.post(`${baseUrl}/api/settings/download-token-ttl`, { minutes: Math.max(1, Math.floor(downloadTokenTtlMinutes)) }, { headers: { Authorization: `Bearer ${token}` } }),
                axios.post(`${baseUrl}/api/settings/server-name`, { name: serverName }, { headers: { Authorization: `Bearer ${token}` } })
            ]);
            setSuccess('所有设置已保存');
        } catch (err) {
            if (axios.isAxiosError(err)) {
                const data = err.response?.data;
                setError(typeof data === 'string' && data ? data : '保存失败');
            } else {
                setError('保存失败');
            }
        } finally {
            setSaving(false);
        }
    };

    if (loading) return <p>加载设置中...</p>;

    return (
        <div className="space-y-6">
            <div className="p-4 border rounded dark:border-gray-700">
                <h3 className="font-medium mb-4 text-lg">基础路径</h3>
                <div className="mb-4">
                    <label className="block text-sm font-medium mb-1">cli-capture 路径</label>
                    <input 
                        type="text" 
                        value={cliPath}
                        onChange={e => setCliPath(e.target.value)}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        placeholder="例如: C:\Program Files\cli-capture\cli-capture.exe"
                    />
                    <p className="text-xs text-gray-500 mt-1">留空将导致录制、推流和硬件探测时报错</p>
                </div>
                <div className="mb-4">
                    <label className="block text-sm font-medium mb-1">全局录像存储根目录</label>
                    <input 
                        type="text" 
                        value={globalPath}
                        onChange={e => setGlobalPath(e.target.value)}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        placeholder="例如: D:\AllRecordings"
                    />
                    <p className="text-xs text-gray-500 mt-1">留空则使用默认路径</p>
                </div>
                <div className="mb-4">
                    <label className="block text-sm font-medium mb-1">后端名称</label>
                    <input 
                        type="text"
                        value={serverName}
                        onChange={e => setServerName(e.target.value)}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        placeholder="例如: 会议室录制主机"
                    />
                </div>
                <div>
                    <label className="block text-sm font-medium mb-1">下载令牌有效期 (分钟)</label>
                    <input 
                        type="number"
                        min={1}
                        value={downloadTokenTtlMinutes}
                        onChange={e => setDownloadTokenTtlMinutes(parseInt(e.target.value) || 0)}
                        className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                    />
                </div>
            </div>

            <div className="p-4 border rounded dark:border-gray-700">
                <h3 className="font-medium mb-4 text-lg">全局录制限制 (默认值)</h3>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div>
                        <label className="block text-sm font-medium mb-1">默认最大分辨率</label>
                        <select 
                            value={recordConfig.max_res}
                            onChange={e => setRecordConfig({...recordConfig, max_res: e.target.value})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        >
                            <option value="4k">4K</option>
                            <option value="1080p">1080p</option>
                            <option value="720p">720p</option>
                        </select>
                    </div>
                    <div>
                        <label className="block text-sm font-medium mb-1">默认视频编码器</label>
                        <input 
                            type="text" 
                            value={recordConfig.video_encoder}
                            onChange={e => setRecordConfig({...recordConfig, video_encoder: e.target.value})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        />
                    </div>
                    <div>
                        <label className="block text-sm font-medium mb-1">默认最大帧率 (FPS)</label>
                        <input 
                            type="number" 
                            value={recordConfig.max_fps}
                            onChange={e => setRecordConfig({...recordConfig, max_fps: parseInt(e.target.value)})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        />
                    </div>
                    <div>
                        <label className="block text-sm font-medium mb-1">默认最大码率 (Kbps)</label>
                        <input 
                            type="number" 
                            value={recordConfig.max_bitrate}
                            onChange={e => setRecordConfig({...recordConfig, max_bitrate: parseInt(e.target.value)})}
                            className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
                        />
                    </div>
                </div>
            </div>

            <button 
                onClick={handleSave} 
                disabled={saving}
                className="bg-green-600 hover:bg-green-700 text-white font-bold py-2 px-6 rounded"
            >
                {saving ? '保存中...' : '保存所有设置'}
            </button>
        </div>
    );
}
