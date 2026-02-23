import { useEffect, useState } from 'react';
import axios from 'axios';
import { useNavigate } from 'react-router-dom';
import PasswordStrengthIndicator from '../components/PasswordStrengthIndicator';

export default function Login() {
  const navigate = useNavigate();
  const [isRegistering, setIsRegistering] = useState(false);
  const [formData, setFormData] = useState({
    username: '',
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

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setFormData({ ...formData, [e.target.name]: e.target.value });
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    if (!backendUrl) {
      setError('未选择后端');
      return;
    }

    if (isRegistering) {
      if (formData.password !== formData.confirmPassword) {
        setError("两次输入的密码不一致");
        return;
      }
    }

    const endpoint = isRegistering ? '/api/auth/register' : '/api/auth/login';
    const payload = {
      username: formData.username,
      password: formData.password
    };

    try {
      const response = await axios.post(`${backendUrl}${endpoint}`, payload);
      const { token, role } = response.data;
      const normalizedRole = typeof role === 'string' ? role.toLowerCase() : role;
      
      // Store token and user info
      localStorage.setItem('token', token);
      localStorage.setItem('role', normalizedRole);
      localStorage.setItem('username', formData.username);

      // Redirect
      navigate('/');
    } catch (err: any) {
      console.error(err);
      // 改进错误信息显示
      const errorMessage = err.response?.data || err.message || '认证失败';
      setError(typeof errorMessage === 'string' ? errorMessage : JSON.stringify(errorMessage));
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100">
      <div className="bg-white dark:bg-gray-800 p-8 rounded-lg shadow-md w-full max-w-md">
        <h1 className="text-2xl font-bold mb-6 text-center">
          {isRegistering ? '注册' : '登录'}
        </h1>
        {backendName && (
          <div className="text-xs text-gray-500 mb-4 text-center">
            当前后端：{backendName}
          </div>
        )}
        
        {error && <div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded mb-4">{error}</div>}

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">用户名</label>
            <input 
              type="text" 
              name="username" 
              value={formData.username} 
              onChange={handleChange} 
              className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" 
              required 
            />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">密码</label>
            <input 
              type="password" 
              name="password" 
              value={formData.password} 
              onChange={handleChange} 
              className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" 
              required 
            />
            {isRegistering && <PasswordStrengthIndicator password={formData.password} />}
          </div>
          
          {isRegistering && (
            <div>
              <label className="block text-sm font-medium mb-1">确认密码</label>
              <input 
                type="password" 
                name="confirmPassword" 
                value={formData.confirmPassword} 
                onChange={handleChange} 
                className="w-full p-2 border rounded dark:bg-gray-700 dark:border-gray-600" 
                required 
              />
            </div>
          )}

          <button type="submit" className="w-full bg-blue-600 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded">
            {isRegistering ? '注册' : '登录'}
          </button>
        </form>

        <div className="mt-4 text-center">
          <button 
            onClick={() => {
              setIsRegistering(!isRegistering);
              setError('');
              setFormData({ username: '', password: '', confirmPassword: '' });
            }}
            className="text-blue-500 hover:underline text-sm"
          >
            {isRegistering ? '已有账号？登录' : '没有账号？注册'}
          </button>
        </div>
        <div className="mt-2 text-center">
          <button onClick={() => navigate('/discover')} className="text-blue-500 hover:underline text-sm">
            切换后端
          </button>
        </div>
      </div>
    </div>
  );
}
