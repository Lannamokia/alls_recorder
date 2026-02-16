import { BrowserRouter, Routes, Route } from 'react-router-dom';
import InitPage from './pages/InitPage';
import Dashboard from './pages/Dashboard';
import Login from './pages/Login';
import AdminDashboard from './components/AdminDashboard';
import BackendDiscovery from './pages/BackendDiscovery';

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/discover" element={<BackendDiscovery />} />
        <Route path="/init" element={<InitPage />} />
        <Route path="/login" element={<Login />} />
        <Route path="/admin" element={<AdminDashboard />} />
        <Route path="/" element={<Dashboard />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
