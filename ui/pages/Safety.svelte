<script>
  import Row from '../lib/Row.svelte';
  import Switch from '../lib/Switch.svelte';
  import Select from '../lib/Select.svelte';
  import { cfg, update } from '../lib/store.svelte.js';

  const rateOptions = $derived(cfg.info.rateOptions.map((n) => ({ value: n, label: `${n} 次` })));
</script>

<h1>安全</h1>

<Row title="處理網路路徑 (\\host\share)">
  <Switch label="處理網路路徑" checked={cfg.settings.allowUnc} onchange={(v) => update({ allowUnc: v })} />
</Row>
{#if cfg.settings.allowUnc}
  <p class="warn">
    光是檢查路徑是否存在，Windows 就會主動連線到對方的 SMB 伺服器，可能洩漏登入雜湊（NTLM hash）。只在你信任剪貼簿內容來源時開啟。
  </p>
{/if}

<Row
  title="每分鐘開啟上限"
  desc="超過上限會自動暫停監控並跳通知，避免有程式瘋狂複製路徑造成視窗洪水。左鍵點系統匣圖示即可恢復。"
>
  <Select
    label="每分鐘開啟上限"
    value={cfg.settings.rateLimitPerMinute}
    options={rateOptions}
    onchange={(v) => update({ rateLimitPerMinute: v })}
  />
</Row>

<style>
  .warn {
    margin: 0 0 4px;
    padding: 10px 16px;
    border-radius: var(--radius);
    background: var(--warn-bg);
    color: var(--warn-text);
    font-size: 12px;
  }
</style>
