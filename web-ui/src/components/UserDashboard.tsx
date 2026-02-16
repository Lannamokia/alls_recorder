import { useState, useEffect } from 'react';
import axios from 'axios';
import FileList from './FileList';
import UserSettingsModal from './UserSettingsModal';
import { Settings } from 'lucide-react';

interface ActiveUser {
  user_id: string;
  username: string;
}

interface StopRequest {
  requester_id: string;
  requester_name: string;
}

// interface SentRequest {
//   requester_id: string;
//   requester_name: string;
//   status: 'Pending' | 'Accepted' | 'Denied';
// }

interface Announcement {
  id: string;
  content: string;
  created_at: string;
}

export default function UserDashboard() {
  const [isRecording, setIsRecording] = useState(false);
  const [taskType, setTaskType] = useState<'idle' | 'record' | 'stream'>('idle');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [statusMsg, setStatusMsg] = useState('');
  const [showSettings, setShowSettings] = useState(false);
  
  const [activeUsers, setActiveUsers] = useState<ActiveUser[]>([]);
  const [notification, setNotification] = useState<StopRequest | null>(null);
  const [announcements, setAnnouncements] = useState<Announcement[]>([]);
  const [sentRequest, setSentRequest] = useState<{ targetId: string, status: string } | null>(null);

  const token = localStorage.getItem('token');
  const baseUrl = localStorage.getItem('backend_url') || 'http://localhost:3000';

  const fetchStatus = async () => {
    try {
      const res = await axios.get(`${baseUrl}/api/recorder/status`, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setIsRecording(res.data.recording);
      setTaskType(res.data.task_type || (res.data.recording ? 'record' : 'idle'));
    } catch (err) {
      console.error(err);
    }
  };

  const fetchActiveUsers = async () => {
    try {
      const res = await axios.get(`${baseUrl}/api/recorder/active`, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setActiveUsers(res.data);
    } catch (err) {
      console.error(err);
    }
  };

  const checkNotifications = async () => {
    try {
      const res = await axios.get(`${baseUrl}/api/recorder/notifications`, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setNotification(res.data); // data is StopRequest or null
    } catch (err) {
      console.error(err);
    }
  };

  const checkAnnouncements = async () => {
    try {
      const res = await axios.get(`${baseUrl}/api/announcements/unread`, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setAnnouncements(res.data);
    } catch (err) {
      console.error(err);
    }
  };

  const checkSentRequestStatus = async () => {
    if (!sentRequest || sentRequest.status === 'Accepted' || sentRequest.status === 'Denied') return;
    try {
        const res = await axios.get(`${baseUrl}/api/recorder/request-status?target_user_id=${sentRequest.targetId}`, {
            headers: { Authorization: `Bearer ${token}` }
        });
        const status = res.data.status; // Pending, Accepted, Denied
        
        if (status !== sentRequest.status) {
            setSentRequest(prev => prev ? { ...prev, status } : null);
            if (status === 'Accepted') {
                setStatusMsg('对方已接受您的停止请求，正在切换录制...');
                // The switch logic happens on backend, we just wait for our recording to start
                // which will be picked up by fetchStatus poll.
            } else if (status === 'Denied') {
                setError('对方拒绝了您的停止请求');
            }
        }
    } catch (err) {
        // Request might be gone or error
        // If 404, maybe it was removed or didn't exist
    }
  };

  useEffect(() => {
    fetchStatus();
    fetchActiveUsers();
    checkNotifications();
    checkAnnouncements();

    const interval = setInterval(() => {
      fetchStatus();
      fetchActiveUsers();
      checkNotifications();
      checkSentRequestStatus();
    }, 2000); // Poll every 2s

    return () => clearInterval(interval);
  }, [sentRequest]); // Add sentRequest dependency to update polling logic if needed

  const handleRequestStop = async (targetUserId: string) => {
      if (!confirm('确定要请求停止该用户的录制吗？')) return;
      try {
          await axios.post(`${baseUrl}/api/recorder/request-stop`, { target_user_id: targetUserId }, {
              headers: { Authorization: `Bearer ${token}` }
          });
          setSentRequest({ targetId: targetUserId, status: 'Pending' });
          setStatusMsg('请求已发送，等待对方响应...');
      } catch (err: any) {
          setError(err.response?.data || '请求发送失败');
      }
  };

  const handleStart = async (mode: 'record' | 'stream') => {
    setLoading(true);
    setError('');
    setStatusMsg('');
    try {
      await axios.post(`${baseUrl}/api/recorder/start`, { mode }, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setIsRecording(true);
      setTaskType(mode);
      setStatusMsg(mode === 'record' ? '已开始录制' : '已开始推流');
    } catch (err: any) {
      console.error(err);
      setError(err.response?.data || '启动失败');
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    setLoading(true);
    setError('');
    setStatusMsg('');
    try {
      await axios.post(`${baseUrl}/api/recorder/stop`, {}, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setIsRecording(false);
      setTaskType('idle');
      setStatusMsg('已停止');
    } catch (err: any) {
      console.error(err);
      setError(err.response?.data || '停止失败');
    } finally {
      setLoading(false);
    }
  };

  const handleRespondStop = async (accept: boolean) => {
    if (!notification) return;
    try {
      await axios.post(`${baseUrl}/api/recorder/respond-stop`, {
        accept,
        requester_id: notification.requester_id
      }, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setNotification(null);
      if (accept) {
        setIsRecording(false);
        setStatusMsg('已接受停止请求');
      }
    } catch (err: any) {
      console.error(err);
      setError(err.response?.data || '响应失败');
    }
  };

  return (
    <div className="space-y-6">
      {/* Status & Controls */}
      <div className="bg-white dark:bg-gray-800 p-6 rounded-lg shadow-md">
        <div className="flex justify-between items-center mb-4">
            <h2 className="text-xl font-semibold">录制状态</h2>
            <button 
                onClick={() => setShowSettings(true)}
                className="flex items-center space-x-1 text-gray-500 hover:text-blue-600"
            >
                <Settings size={20} />
                <span>录制设置</span>
            </button>
        </div>
        
        {error && <div className="bg-red-100 text-red-700 p-3 rounded mb-4">{error}</div>}
        {statusMsg && <div className="bg-green-100 text-green-700 p-3 rounded mb-4">{statusMsg}</div>}

        <div className="flex items-center space-x-4 mb-6">
          <div className={`w-4 h-4 rounded-full ${isRecording ? 'bg-red-500 animate-pulse' : 'bg-gray-400'}`}></div>
          <span className="font-medium">
            {isRecording ? (taskType === 'stream' ? '正在推流' : '正在录制') : '空闲'}
          </span>
        </div>

        <div className="flex space-x-4">
          {!isRecording ? (
            <>
              <button 
                onClick={() => handleStart('record')} 
                disabled={loading}
                className="bg-red-600 hover:bg-red-700 text-white font-bold py-2 px-6 rounded"
              >
                {loading ? '启动中...' : '开始录制'}
              </button>
              <button 
                onClick={() => handleStart('stream')} 
                disabled={loading}
                className="bg-purple-600 hover:bg-purple-700 text-white font-bold py-2 px-6 rounded"
              >
                {loading ? '启动中...' : '开始推流'}
              </button>
            </>
          ) : (
            <button 
              onClick={handleStop} 
              disabled={loading}
              className="bg-gray-600 hover:bg-gray-700 text-white font-bold py-2 px-6 rounded"
            >
              {loading ? '停止中...' : (taskType === 'stream' ? '停止推流' : '停止录制')}
            </button>
          )}
        </div>

        {/* Notification Modal/Alert */}
        {notification && (
          <div className="mt-6 border-l-4 border-yellow-500 bg-yellow-100 p-4 dark:bg-yellow-900 dark:text-yellow-100">
            <p className="font-bold">管理员请求停止录制</p>
            <p className="mb-2">管理员 {notification.requester_name} 请求您停止录制。</p>
            <div className="flex space-x-3">
              <button 
                onClick={() => handleRespondStop(true)}
                className="bg-green-600 hover:bg-green-700 text-white px-3 py-1 rounded text-sm"
              >
                接受
              </button>
              <button 
                onClick={() => handleRespondStop(false)}
                className="bg-red-600 hover:bg-red-700 text-white px-3 py-1 rounded text-sm"
              >
                拒绝
              </button>
            </div>
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        {/* Active Users */}
        <div className="bg-white dark:bg-gray-800 p-6 rounded-lg shadow-md">
          <h3 className="text-lg font-semibold mb-3">在线用户</h3>
          {activeUsers.length === 0 ? (
            <p className="text-gray-500">无其他在线用户</p>
          ) : (
            <ul className="space-y-2">
              {activeUsers.map(u => (
                <li key={u.user_id} className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    <div className="w-2 h-2 bg-green-500 rounded-full"></div>
                    <span>{u.username}</span>
                  </div>
                  {/* Avoid requesting stop for yourself */}
                  <button 
                    onClick={() => handleRequestStop(u.user_id)}
                    className="text-xs bg-orange-100 text-orange-600 px-2 py-1 rounded hover:bg-orange-200"
                  >
                    请求停止
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        {/* Announcements */}
        <div className="bg-white dark:bg-gray-800 p-6 rounded-lg shadow-md md:col-span-2">
          <h3 className="text-lg font-semibold mb-3">公告</h3>
          {announcements.length === 0 ? (
            <p className="text-gray-500">暂无公告</p>
          ) : (
            <div className="space-y-4">
              {announcements.map(a => (
                <div key={a.id} className="border-b pb-2 dark:border-gray-700 last:border-0">
                  <p className="text-sm text-gray-400 mb-1">{new Date(a.created_at).toLocaleString()}</p>
                  <p>{a.content}</p>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* File List */}
      <FileList />

      {/* Modals */}
      <UserSettingsModal isOpen={showSettings} onClose={() => setShowSettings(false)} />
    </div>
  );
}
