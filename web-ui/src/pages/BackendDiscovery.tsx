import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';

type BackendInfo = {
  baseUrl: string;
  name: string;
  initialized: boolean;
};

export default function BackendDiscovery() {
  const navigate = useNavigate();
  const [found, setFound] = useState<BackendInfo[]>([]);
  const [error, setError] = useState('');
  const [manualInput, setManualInput] = useState('');
  const [scanPrefix, setScanPrefix] = useState('192.168.1.');
  const [scanStart, setScanStart] = useState(1);
  const [scanEnd, setScanEnd] = useState(50);
  const [scanning, setScanning] = useState(false);

  const [candidates, setCandidates] = useState<string[]>(() => {
    const raw = localStorage.getItem('backend_candidates');
    try {
      const list = JSON.parse(raw || '[]') as string[];
      return list.filter(Boolean);
    } catch {
      return [];
    }
  });
  const candidatesRef = useRef<string[]>(candidates);
  const lastProbeRef = useRef<Map<string, number>>(new Map());

  const persistCandidates = useCallback((list: string[]) => {
    localStorage.setItem('backend_candidates', JSON.stringify(list));
  }, []);

  const normalizeBaseUrl = useCallback((input: string) => {
    let v = input.trim();
    if (!v) return '';
    if (!/^https?:\/\//i.test(v)) v = `http://${v}`;
    try {
      const u = new URL(v);
      if (!u.port) u.port = '3000';
      return u.origin;
    } catch {
      return '';
    }
  }, []);

  const getCanonicalKey = useCallback((baseUrl: string) => {
    try {
      const u = new URL(baseUrl);
      const host = u.hostname.toLowerCase();
      const port = u.port || '3000';
      if (host === 'localhost' || host === '127.0.0.1' || host === '::1') {
        return `local:${port}`;
      }
      return `${host}:${port}`;
    } catch {
      return baseUrl;
    }
  }, []);

  const addFound = useCallback((info: BackendInfo) => {
    setFound(prev => {
      const key = getCanonicalKey(info.baseUrl);
      if (prev.some(item => getCanonicalKey(item.baseUrl) === key)) return prev;
      return [...prev, info];
    });
    setCandidates(prev => {
      const nextCandidates = Array.from(
        new Map(
          [...prev, info.baseUrl].map(item => [getCanonicalKey(item), item])
        ).values()
      );
      candidatesRef.current = nextCandidates;
      persistCandidates(nextCandidates);
      return nextCandidates;
    });
  }, [getCanonicalKey, persistCandidates]);

  const probeBackend = useCallback(async (baseUrl: string) => {
    const now = Date.now();
    const last = lastProbeRef.current.get(baseUrl) || 0;
    if (now - last < 3000) return;
    lastProbeRef.current.set(baseUrl, now);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 1200);
    try {
      const res = await fetch(`${baseUrl}/api/setup/info`, { signal: controller.signal });
      if (!res.ok) return;
      const data = await res.json();
      const name = typeof data?.name === 'string' ? data.name : baseUrl;
      const initialized = Boolean(data?.initialized);
      addFound({ baseUrl, name, initialized });
    } catch (e) {
      void e;
    } finally {
      clearTimeout(timer);
    }
  }, [addFound]);

  const quickDiscover = useCallback(async () => {
    setError('');
    const bases = new Set<string>();
    const currentHost = window.location.hostname;
    if (currentHost) bases.add(normalizeBaseUrl(currentHost));
    bases.add(normalizeBaseUrl('localhost'));
    bases.add(normalizeBaseUrl('127.0.0.1'));
    const savedBackend = localStorage.getItem('backend_url');
    if (savedBackend) bases.add(normalizeBaseUrl(savedBackend));
    candidatesRef.current.forEach(v => bases.add(normalizeBaseUrl(v)));
    const list = Array.from(bases).filter(Boolean);
    await Promise.all(list.map(b => probeBackend(b)));
  }, [normalizeBaseUrl, probeBackend]);

  const handleSelect = (info: BackendInfo) => {
    localStorage.setItem('backend_url', info.baseUrl);
    localStorage.setItem('backend_name', info.name);
    localStorage.removeItem('token');
    localStorage.removeItem('role');
    localStorage.removeItem('username');
    navigate(info.initialized ? '/login' : '/init');
  };

  const handleManualAdd = async () => {
    setError('');
    const baseUrl = normalizeBaseUrl(manualInput);
    if (!baseUrl) {
      setError('地址格式不正确');
      return;
    }
    await probeBackend(baseUrl);
  };

  const scanSubnet = async () => {
    setError('');
    const start = Math.max(1, Math.min(254, scanStart));
    const end = Math.max(1, Math.min(254, scanEnd));
    if (!scanPrefix.trim().endsWith('.')) {
      setError('网段前缀需以 . 结尾');
      return;
    }
    setScanning(true);
    const targets: string[] = [];
    const from = Math.min(start, end);
    const to = Math.max(start, end);
    for (let i = from; i <= to; i += 1) {
      targets.push(`http://${scanPrefix}${i}:3000`);
    }
    const limit = 20;
    let index = 0;
    const workers = new Array(limit).fill(0).map(async () => {
      while (index < targets.length) {
        const baseUrl = targets[index];
        index += 1;
        await probeBackend(baseUrl);
      }
    });
    await Promise.all(workers);
    setScanning(false);
  };

  useEffect(() => {
    quickDiscover();
  }, [quickDiscover]);

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100">
      <div className="bg-white dark:bg-gray-800 p-8 rounded-lg shadow-md w-full max-w-2xl">
        <h1 className="text-2xl font-bold mb-4 text-center">发现后端</h1>
        {error && <div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded mb-4">{error}</div>}
        <div className="space-y-4">
          <div className="flex gap-2">
            <input
              type="text"
              placeholder="输入后端地址或 IP"
              value={manualInput}
              onChange={e => setManualInput(e.target.value)}
              className="flex-1 p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
            />
            <button onClick={handleManualAdd} className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded">
              添加
            </button>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-4 gap-2">
            <input
              type="text"
              value={scanPrefix}
              onChange={e => setScanPrefix(e.target.value)}
              className="p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
            />
            <input
              type="number"
              min={1}
              max={254}
              value={scanStart}
              onChange={e => setScanStart(parseInt(e.target.value) || 1)}
              className="p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
            />
            <input
              type="number"
              min={1}
              max={254}
              value={scanEnd}
              onChange={e => setScanEnd(parseInt(e.target.value) || 50)}
              className="p-2 border rounded dark:bg-gray-700 dark:border-gray-600"
            />
            <button
              onClick={scanSubnet}
              disabled={scanning}
              className="px-4 py-2 bg-gray-700 hover:bg-gray-800 text-white rounded disabled:opacity-50"
            >
              {scanning ? '扫描中...' : '扫描网段'}
            </button>
          </div>
          <div>
            <button onClick={quickDiscover} className="text-sm text-blue-500 hover:underline">
              重新发现
            </button>
          </div>
          <div className="border rounded dark:border-gray-700 divide-y dark:divide-gray-700">
            {found.length === 0 ? (
              <div className="p-4 text-sm text-gray-500">未发现后端，可手动添加或扫描网段</div>
            ) : (
              found.map(item => (
                <div key={item.baseUrl} className="p-4 flex items-center justify-between">
                  <div>
                    <div className="font-medium flex items-center gap-2">
                      <span>{item.name}</span>
                      {!item.initialized && (
                        <span className="text-xs px-2 py-0.5 rounded bg-yellow-100 text-yellow-800 dark:bg-yellow-900/40 dark:text-yellow-200">
                          未初始化
                        </span>
                      )}
                    </div>
                    <div className="text-xs text-gray-500">{item.baseUrl}</div>
                    <div className="text-xs text-gray-500">{item.initialized ? '已初始化' : '未初始化'}</div>
                  </div>
                  <button
                    onClick={() => handleSelect(item)}
                    className="px-3 py-1 bg-green-600 hover:bg-green-700 text-white rounded"
                  >
                    {item.initialized ? '选择' : '去初始化'}
                  </button>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
