<script>
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import openIcon from '@material-symbols/svg-400/outlined/open_in_new.svg?raw';

  let msg = $state({ label: '', name: '' });
  // 連續觸發時重新播放進場動畫
  let n = $state(0);

  onMount(() => {
    const sub = listen('toast', (e) => {
      msg = e.payload;
      n++;
    });
    return () => sub.then((off) => off());
  });
</script>

{#key n}
  <div class="toast">
    <span class="icon">{@html openIcon}</span>
    <div class="text">
      <div class="label">{msg.label}</div>
      <div class="name">{msg.name}</div>
    </div>
  </div>
{/key}

<style>
  /* 視窗本身是透明的，只有卡片可見 */
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 12px;
    height: calc(100vh - 8px);
    margin: 4px;
    padding: 0 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    animation: in 0.18s ease-out;
  }
  .icon :global(svg) {
    display: block;
    width: 24px;
    height: 24px;
    fill: var(--accent);
  }
  .text {
    min-width: 0;
  }
  .label {
    font-size: 12px;
    color: var(--text-dim);
  }
  .name {
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
</style>
