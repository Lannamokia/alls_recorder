import { useState, useEffect } from 'react';
import axios from 'axios';
import { Trash2, Edit2, FileVideo, Check, X, Download } from 'lucide-react';

interface RecordingFile {
  id: string;
  filename: string;
  status: string;
  created_at: string;
}

export default function FileList() {
  const [files, setFiles] = useState<RecordingFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [newName, setNewName] = useState('');

  const token = localStorage.getItem('token');
  const baseUrl = localStorage.getItem('backend_url') || 'http://localhost:3000';

  const fetchFiles = async () => {
    setLoading(true);
    try {
      const res = await axios.get(`${baseUrl}/api/files`, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setFiles(res.data);
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchFiles();
    const interval = setInterval(fetchFiles, 10000); // Auto refresh
    return () => clearInterval(interval);
  }, []);

  const handleDelete = async (id: string) => {
    if (!confirm('确定要删除此文件吗？')) return;
    try {
      await axios.delete(`${baseUrl}/api/files/${id}`, {
        headers: { Authorization: `Bearer ${token}` }
      });
      fetchFiles();
    } catch (err) {
      console.error(err);
      alert('删除失败');
    }
  };

  const handleDownload = async (file: RecordingFile) => {
    try {
      const tokenRes = await axios.post(`${baseUrl}/api/files/${file.id}/download-token`, {}, {
        headers: { Authorization: `Bearer ${token}` }
      });
      const downloadToken = tokenRes.data?.token as string | undefined;
      if (!downloadToken) {
        alert('下载失败');
        return;
      }
      const res = await axios.get(`${baseUrl}/api/files/download?token=${encodeURIComponent(downloadToken)}`, {
        headers: { Authorization: `Bearer ${token}` },
        responseType: 'blob'
      });
      const blobUrl = window.URL.createObjectURL(res.data);
      const link = document.createElement('a');
      link.href = blobUrl;
      link.download = file.filename;
      document.body.appendChild(link);
      link.click();
      link.remove();
      window.URL.revokeObjectURL(blobUrl);
    } catch (err) {
      console.error(err);
      alert('下载失败');
    }
  };

  const handleRename = async (id: string) => {
    if (!newName.trim()) return;
    try {
      await axios.post(`${baseUrl}/api/files/${id}/rename`, { new_filename: newName }, {
        headers: { Authorization: `Bearer ${token}` }
      });
      setEditingId(null);
      setNewName('');
      fetchFiles();
    } catch (err) {
      console.error(err);
      alert('重命名失败');
    }
  };

  return (
    <div className="bg-white dark:bg-gray-800 p-6 rounded-lg shadow-md mt-6">
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-xl font-semibold">录像文件</h2>
        <button onClick={fetchFiles} className="text-sm text-blue-500 hover:underline">刷新</button>
      </div>

      {loading && files.length === 0 ? (
        <p>加载文件中...</p>
      ) : files.length === 0 ? (
        <p className="text-gray-500">暂无录像文件</p>
      ) : (
        <div className="space-y-2">
          {files.map(file => (
            <div key={file.id} className="flex items-center justify-between p-3 border rounded dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-700">
              <div className="flex items-center space-x-3">
                <FileVideo className="text-gray-500" />
                <div>
                  {editingId === file.id ? (
                    <div className="flex items-center space-x-2">
                        <input 
                        type="text" 
                        value={newName} 
                        onChange={e => setNewName(e.target.value)} 
                        className="border rounded p-1 text-sm dark:bg-gray-600"
                        autoFocus
                        />
                        <button onClick={() => handleRename(file.id)} className="text-green-500"><Check size={16}/></button>
                        <button onClick={() => setEditingId(null)} className="text-red-500"><X size={16}/></button>
                    </div>
                  ) : (
                    <p className="font-medium">{file.filename}</p>
                  )}
                  <p className="text-xs text-gray-500">
                    {new Date(file.created_at).toLocaleString()} - <span className={`uppercase text-[10px] px-1 rounded ${file.status === 'recording' ? 'bg-red-100 text-red-800' : 'bg-green-100 text-green-800'}`}>{file.status}</span>
                  </p>
                </div>
              </div>
              
              <div className="flex space-x-2">
                {editingId !== file.id && (
                    <button onClick={() => { setEditingId(file.id); setNewName(file.filename); }} className="text-gray-500 hover:text-blue-500">
                        <Edit2 size={18} />
                    </button>
                )}
                <button onClick={() => handleDownload(file)} className="text-gray-500 hover:text-blue-500">
                  <Download size={18} />
                </button>
                <button onClick={() => handleDelete(file.id)} className="text-gray-500 hover:text-red-500">
                  <Trash2 size={18} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
