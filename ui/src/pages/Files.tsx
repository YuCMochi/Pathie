import { Row } from '../components/Row';
import { Select, type Option } from '../components/ui/select';
import { Switch } from '../components/ui/switch';
import { update, useStore, type Settings } from '../lib/store';

const fileActions: Option<Settings['fileAction']>[] = [
  { value: 'ignore', label: '不處理' },
  { value: 'reveal', label: '在資料夾中選取' },
  { value: 'open', label: '直接開啟（執行檔除外）' },
];

export function Files() {
  const { settings } = useStore();
  return (
    <>
      <h1>檔案與網址</h1>

      <h2>檔案</h2>
      <Row
        title="複製檔案路徑時"
        desc="資料夾一律直接開啟。「直接開啟」時，.exe、.bat、.ps1、.lnk 等可執行或腳本類檔案仍只會選取、不會執行。"
      >
        <Select
          label="複製檔案路徑時"
          value={settings.fileAction}
          options={fileActions}
          onChange={(v) => update({ fileAction: v })}
        />
      </Row>

      <h2>網址</h2>
      <Row title="自動開啟網址" desc="只認 http://、https:// 開頭，或 www. 開頭（自動補上 https://）。">
        <Switch
          aria-label="自動開啟網址"
          checked={settings.openUrl}
          onCheckedChange={(v) => update({ openUrl: v })}
        />
      </Row>
      <Row
        title="從瀏覽器複製的網址不開啟"
        desc="複製當下前景視窗是瀏覽器就略過：Chrome、Edge、Firefox、Brave、Opera、Vivaldi、Zen、Arc、Dia。"
        disabled={!settings.openUrl}
      >
        <Switch
          aria-label="從瀏覽器複製的網址不開啟"
          checked={settings.skipUrlFromBrowser}
          disabled={!settings.openUrl}
          onCheckedChange={(v) => update({ skipUrlFromBrowser: v })}
        />
      </Row>
    </>
  );
}
