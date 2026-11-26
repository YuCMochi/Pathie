<script>
  import Row from '../lib/Row.svelte';
  import Switch from '../lib/Switch.svelte';
  import Select from '../lib/Select.svelte';
  import { cfg, update } from '../lib/store.svelte.js';

  const fileActions = [
    { value: 'ignore', label: '不處理' },
    { value: 'reveal', label: '在資料夾中選取' },
    { value: 'open', label: '直接開啟（執行檔除外）' },
  ];
</script>

<h1>檔案與網址</h1>

<h2>檔案</h2>
<Row title="複製檔案路徑時" desc="資料夾一律直接開啟。「直接開啟」時，.exe、.bat、.ps1、.lnk 等可執行或腳本類檔案仍只會選取、不會執行。">
  <Select
    label="複製檔案路徑時"
    value={cfg.settings.fileAction}
    options={fileActions}
    onchange={(v) => update({ fileAction: v })}
  />
</Row>

<h2>網址</h2>
<Row title="自動開啟網址" desc="只認 http://、https:// 開頭，或 www. 開頭（自動補上 https://）。">
  <Switch label="自動開啟網址" checked={cfg.settings.openUrl} onchange={(v) => update({ openUrl: v })} />
</Row>
<Row
  title="從瀏覽器複製的網址不開啟"
  desc="複製當下前景視窗是瀏覽器就略過：Chrome、Edge、Firefox、Brave、Opera、Vivaldi、Zen、Arc、Dia。"
  disabled={!cfg.settings.openUrl}
>
  <Switch
    label="從瀏覽器複製的網址不開啟"
    checked={cfg.settings.skipUrlFromBrowser}
    disabled={!cfg.settings.openUrl}
    onchange={(v) => update({ skipUrlFromBrowser: v })}
  />
</Row>
