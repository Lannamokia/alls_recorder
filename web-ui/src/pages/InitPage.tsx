import { useEffect, useState } from 'react';
import axios from 'axios';
import { useNavigate } from 'react-router-dom';

export default function InitPage() {
  const navigate = useNavigate();
  const [step, setStep] = useState(1);
  const [dbConfig, setDbConfig] = useState({ 
    host: 'localhost', 
    port: 5432, 
    user: 'postgres', 
    password: '', 
    dbname: 'alls_recorder' 
  });
  const [adminConfig, setAdminConfig] = useState({ 
    username: 'admin', 
    password: '', 
    confirmPassword: '' 
  });
  const [error, setError] = useState('');
  const backendUrl = localStorage.getItem('backend_url');
  const backendName = localStorage.getItem('backend_name');

  useEffect(() => {
    if (!backendUrl) {
      navigate('/discover');
    }
  }, [backendUrl, navigate]);

  const handleDbSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!backendUrl) {
      setError('未选择后端');
      return;
    }
    try {
      await axios.post(`${backendUrl}/api/setup/db`, dbConfig);
      setStep(2);
      setError('');
    } catch (err: any) {
      console.error(err);
      setError(err.response?.data || '数据库连接失败');
    }
  };

  const handleAdminSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (adminConfig.password !== adminConfig.confirmPassword) {
      setError("两次输入的密码不一致");
      return;
    }
    if (!backendUrl) {
      setError('未选择后端');
      return;
    }
    try {
      await axios.post(`${backendUrl}/api/setup/admin`, { 
        username: adminConfig.username, 
        password: adminConfig.password 
      });
      navigate('/login');
    } catch (err: any) {
      console.error(err);
      setError(err.response?.data || '创建管理员失败');
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100">
      <div className="bg-white dark:bg-gray-800 p-8 rounded-lg shadow-md w-full max-w-md">
        <h1 className="text-2xl font-bold mb-6 text-center">系统初始化</h1>
        {backendName && (
          <div className="text-xs text-gray-500 mb-4 text-center">
            当前后端：{backendName}
          </div>
        )}
        {error && <div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded mb-4">{error}</div>}
        
        {step === 1 ? (
          <form onSubmit={handleDbSubmit} className="space-y-4">
            <h2 className="text-xl font-semibold">第一步：数据库配置</h2>
            <div>
              <label className="block text-sm font-medium mb-1">主机地址</label>
              <input type="text" value={dbConfig.host} onChange={e => setDbConfig({...dbConfig, host: e.target.value})} className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" required />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">端口</label>
              <input type="number" value={dbConfig.port} onChange={e => setDbConfig({...dbConfig, port: parseInt(e.target.value)})} className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" required />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">用户名</label>
              <input type="text" value={dbConfig.user} onChange={e => setDbConfig({...dbConfig, user: e.target.value})} className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" required />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">密码</label>
              <input type="password" value={dbConfig.password} onChange={e => setDbConfig({...dbConfig, password: e.target.value})} className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">数据库名称</label>
              <input type="text" value={dbConfig.dbname} onChange={e => setDbConfig({...dbConfig, dbname: e.target.value})} className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" required />
            </div>
            <button type="submit" className="w-full bg-blue-600 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded">下一步</button>
          </form>
        ) : (
          <form onSubmit={handleAdminSubmit} className="space-y-4">
            <h2 className="text-xl font-semibold">第二步：管理员账户</h2>
            <div>
              <label className="block text-sm font-medium mb-1">用户名</label>
              <input type="text" value={adminConfig.username} onChange={e => setAdminConfig({...adminConfig, username: e.target.value})} className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" required />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">密码</label>
              <input type="password" value={adminConfig.password} onChange={e => setAdminConfig({...adminConfig, password: e.target.value})} className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" required />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">确认密码</label>
              <input type="password" value={adminConfig.confirmPassword} onChange={e => setAdminConfig({...adminConfig, confirmPassword: e.target.value})} className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" required />
            </div>
            <button type="submit" className="w-full bg-green-600 hover:bg-green-700 text-white font-bold py-2 px-4 rounded">完成设置</button>
          </form>
        )}
      </div>
    </div>
  );
}
