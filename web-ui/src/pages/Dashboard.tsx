import { useEffect, useState } from 'react';
import axios from 'axios';
import { useNavigate } from 'react-router-dom';
import UserDashboard from '../components/UserDashboard';
import AdminDashboard from '../components/AdminDashboard';

export default function Dashboard() {
  const navigate = useNavigate();
  const [loading, setLoading] = useState(true);
  const [role, setRole] = useState<string | null>(null);
  const backendUrl = localStorage.getItem('backend_url');
  const backendName = localStorage.getItem('backend_name');

  useEffect(() => {
    if (!backendUrl) {
      navigate('/discover');
      return;
    }
    axios.get(`${backendUrl}/api/setup/status`)
      .then(res => {
        if (!res.data.initialized) {
          navigate('/init');
        } else {
          // Check login
          const token = localStorage.getItem('token');
          const savedRole = localStorage.getItem('role');
          
          if (!token) {
            navigate('/login');
          } else {
            setRole(savedRole ? savedRole.toLowerCase() : savedRole);
            setLoading(false);
          }
        }
      })
      .catch((err) => {
        console.error("Status check failed", err);
        setLoading(false);
      });
  }, [navigate, backendUrl]);

  const handleLogout = () => {
    localStorage.removeItem('token');
    localStorage.removeItem('role');
    localStorage.removeItem('username');
    navigate('/login');
  };

  if (loading) return <div className="flex h-screen items-center justify-center">加载中...</div>;

  return (
    <div className="min-h-screen bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100 flex flex-col">
      <nav className="bg-white dark:bg-gray-800 shadow px-6 py-4">
        <div className="max-w-7xl mx-auto flex justify-between items-center w-full">
          <div>
            <h1 className="text-xl font-bold">Alls Recorder</h1>
            {backendName && <div className="text-xs text-gray-500">{backendName}</div>}
          </div>
          <div className="flex items-center space-x-4">
              <span className="text-sm text-gray-500 capitalize">{role}</span>
              <button onClick={handleLogout} className="text-sm text-red-500 hover:text-red-700">退出登录</button>
          </div>
        </div>
      </nav>

      <main className="flex-1 p-6 w-full max-w-7xl mx-auto">
        {role === 'admin' && (
          <div className="mb-8">
            <AdminDashboard />
          </div>
        )}
        
        {/* Both admins and users can record */}
        <UserDashboard />
      </main>
    </div>
  );
}
