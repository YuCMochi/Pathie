import { Row } from '../components/Row';
import { Select } from '../components/ui/select';
import { Switch } from '../components/ui/switch';
import { update, useStore } from '../lib/store';

export function Safety() {
  const { settings, info } = useStore();
  const rateOptions = info.rateOptions.map((n) => ({ value: String(n), label: `${n} 次` }));
  return (
    <>
      <h1>安全</h1>

      <Row title="處理網路路徑 (\\host\share)">
        <Switch
          aria-label="處理網路路徑"
          checked={settings.allowUnc}
          onCheckedChange={(v) => update({ allowUnc: v })}
        />
      </Row>
      {settings.allowUnc && (
        <p className="mb-1 rounded-lg bg-warn px-4 py-2.5 text-xs text-warn-foreground">
          光是檢查路徑是否存在，Windows 就會主動連線到對方的 SMB 伺服器，可能洩漏登入雜湊（NTLM hash）。只在你信任剪貼簿內容來源時開啟。
        </p>
      )}

      <Row
        title="每分鐘開啟上限"
        desc="超過上限會自動暫停監控並跳通知，避免有程式瘋狂複製路徑造成視窗洪水。左鍵點系統匣圖示即可恢復。"
      >
        <Select
          label="每分鐘開啟上限"
          value={String(settings.rateLimitPerMinute)}
          options={rateOptions}
          onChange={(v) => update({ rateLimitPerMinute: Number(v) })}
        />
      </Row>
    </>
  );
}
