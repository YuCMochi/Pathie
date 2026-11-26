<script>
  import Row from '../lib/Row.svelte';
  import Switch from '../lib/Switch.svelte';
  import { cfg, setMonitoring, setStartup } from '../lib/store.svelte.js';
</script>

<h1>一般</h1>
<Row title="監控剪貼簿" desc="關閉後不會偵測複製的內容。也可以左鍵點系統匣圖示快速切換。">
  <Switch label="監控剪貼簿" checked={cfg.monitoring} onchange={setMonitoring} />
</Row>
<Row title="開機自動啟動" desc="登入 Windows 時自動在系統匣執行。">
  <Switch label="開機自動啟動" checked={cfg.startup} onchange={setStartup} />
</Row>
