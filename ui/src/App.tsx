import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { BookOpen, Bell, FolderOpen, Info, Shield, SlidersHorizontal } from 'lucide-react';
import { load, syncMonitoring, uiReady, useStore } from './lib/store';
import { cn } from './lib/utils';
import { General } from './pages/General';
import { Files } from './pages/Files';
import { Safety } from './pages/Safety';
import { Notify } from './pages/Notify';
import { Guide } from './pages/Guide';
import { About } from './pages/About';

const pages = [
  { id: 'general', label: '一般', icon: SlidersHorizontal, view: General },
  { id: 'files', label: '檔案與網址', icon: FolderOpen, view: Files },
  { id: 'safety', label: '安全', icon: Shield, view: Safety },
  { id: 'notify', label: '提示', icon: Bell, view: Notify },
  { id: 'guide', label: '使用說明', icon: BookOpen, view: Guide },
  { id: 'about', label: '關於', icon: Info, view: About },
];

export function App() {
  const { ready } = useStore();
  const [page, setPage] = useState('general');
  const Current = pages.find((p) => p.id === page)!.view;

  useEffect(() => {
    load()
      .then(uiReady)
      .then((p) => p && setPage(p));
    const subs = [
      // 後端要求切換分頁（例如首次啟動直接開到使用說明）
      listen<string>('navigate', (e) => setPage(e.payload)),
      // 左鍵點系統匣、頻率上限自動暫停時，同步開關狀態
      listen<boolean>('monitoring-changed', (e) => syncMonitoring(e.payload)),
    ];
    return () => subs.forEach((s) => s.then((off) => off()));
  }, []);

  return (
    <div className="flex h-screen">
      <nav className="flex w-55 flex-none flex-col gap-0.5 px-2 py-4">
        <div className="px-3 pt-1 pb-4 text-base font-semibold">Pathie</div>
        {pages.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => setPage(id)}
            className={cn(
              'relative flex items-center gap-3 rounded-md px-3 py-2.5 text-left hover:bg-muted',
              page === id &&
                'bg-muted before:absolute before:inset-y-2.5 before:left-0 before:w-[3px] before:rounded-sm before:bg-primary',
            )}
          >
            <Icon className="size-5" />
            {label}
          </button>
        ))}
      </nav>
      <main className="flex-1 overflow-y-auto px-8 py-6">{ready && <Current />}</main>
    </div>
  );
}
