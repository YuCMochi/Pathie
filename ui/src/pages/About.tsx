import { useState } from 'react';
import { checkUpdate, installUpdate, openRepository, useStore, type UpdateStatus } from '../lib/store';

type Phase =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'latest' }
  | { kind: 'found'; update: UpdateStatus }
  | { kind: 'installing'; update: UpdateStatus }
  | { kind: 'error'; message: string };

export function About() {
  const { info } = useStore();
  const [phase, setPhase] = useState<Phase>({ kind: 'idle' });

  const check = async () => {
    setPhase({ kind: 'checking' });
    try {
      const update = await checkUpdate();
      setPhase(update ? { kind: 'found', update } : { kind: 'latest' });
    } catch (e) {
      setPhase({ kind: 'error', message: String(e) });
    }
  };

  const install = async (update: UpdateStatus) => {
    if (update.installable) setPhase({ kind: 'installing', update });
    try {
      await installUpdate();
    } catch (e) {
      setPhase({ kind: 'error', message: String(e) });
    }
  };

  const link = 'text-primary underline disabled:no-underline disabled:opacity-50';
  return (
    <>
      <h1>關於</h1>
      <p>
        <b>Pathie</b> v{info.version}
      </p>
      <p>
        {phase.kind === 'found' ? (
          <>
            有新版本 v{phase.update.version}{' '}
            <button className={link} onClick={() => install(phase.update)}>
              {phase.update.installable ? '下載並更新' : '前往下載頁'}
            </button>
          </>
        ) : phase.kind === 'installing' ? (
          <>正在下載 v{phase.update.version}，完成後會自動重新啟動…</>
        ) : (
          <>
            <button className={link} disabled={phase.kind === 'checking'} onClick={check}>
              {phase.kind === 'checking' ? '檢查中…' : '檢查更新'}
            </button>
            {phase.kind === 'latest' && <span className="text-muted-foreground"> 已是最新版本</span>}
            {phase.kind === 'error' && <span className="text-muted-foreground"> {phase.message}</span>}
          </>
        )}
      </p>
      <p>作者：{info.author}</p>
      <p>
        <button className="text-primary underline" onClick={openRepository}>
          {info.repository}
        </button>
      </p>
    </>
  );
}
