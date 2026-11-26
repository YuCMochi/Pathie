<script>
  import Row from '../lib/Row.svelte';
  import Switch from '../lib/Switch.svelte';
  import { cfg, update } from '../lib/store.svelte.js';
</script>

<h1>提示</h1>
<Row title="開啟成功時顯示提示" desc="Pathie 開啟路徑或網址後，在螢幕右下角短暫顯示「由 Pathie 開啟」，讓你知道剛剛跳出來的視窗是從哪來的。">
  <Switch label="開啟成功時顯示提示" checked={cfg.settings.showToast} onchange={(v) => update({ showToast: v })} />
</Row>
