import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { listen } from '@tauri-apps/api/event';
import { ExternalLink } from 'lucide-react';
import './index.css';

function Toast() {
  const [msg, setMsg] = useState({ label: '', name: '' });
  // 連續觸發時用 key 重新播放進場動畫
  const [n, setN] = useState(0);

  useEffect(() => {
    const sub = listen<{ label: string; name: string }>('toast', (e) => {
      setMsg(e.payload);
      setN((x) => x + 1);
    });
    return () => void sub.then((off) => off());
  }, []);

  return (
    <div
      key={n}
      className="m-1 flex h-[calc(100vh-8px)] animate-[toast-in_0.18s_ease-out] items-center gap-3 rounded-lg border border-border bg-card px-4"
    >
      <ExternalLink className="size-6 shrink-0 text-primary" />
      <div className="min-w-0">
        <div className="text-xs text-muted-foreground">{msg.label}</div>
        <div className="truncate font-semibold">{msg.name}</div>
      </div>
    </div>
  );
}

// 視窗本身是透明的，只有卡片可見
document.documentElement.style.background = 'transparent';
document.body.style.cssText = 'background:transparent;overflow:hidden';
createRoot(document.getElementById('app')!).render(<Toast />);
