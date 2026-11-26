<script>
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { cfg, load } from './lib/store.svelte.js';
  import General from './pages/General.svelte';
  import Files from './pages/Files.svelte';
  import Safety from './pages/Safety.svelte';
  import Notify from './pages/Notify.svelte';
  import Guide from './pages/Guide.svelte';
  import About from './pages/About.svelte';
  import tune from '@material-symbols/svg-400/outlined/tune.svg?raw';
  import folder from '@material-symbols/svg-400/outlined/folder_open.svg?raw';
  import shield from '@material-symbols/svg-400/outlined/shield.svg?raw';
  import bell from '@material-symbols/svg-400/outlined/notifications.svg?raw';
  import book from '@material-symbols/svg-400/outlined/menu_book.svg?raw';
  import info from '@material-symbols/svg-400/outlined/info.svg?raw';

  const pages = [
    { id: 'general', label: '一般', icon: tune, view: General },
    { id: 'files', label: '檔案與網址', icon: folder, view: Files },
    { id: 'safety', label: '安全', icon: shield, view: Safety },
    { id: 'notify', label: '提示', icon: bell, view: Notify },
    { id: 'guide', label: '使用說明', icon: book, view: Guide },
    { id: 'about', label: '關於', icon: info, view: About },
  ];

  let page = $state('general');
  const current = $derived(pages.find((p) => p.id === page));

  onMount(() => {
    load();
    const subs = [
      // 後端要求切換分頁（例如首次啟動直接開到使用說明）
      listen('navigate', (e) => (page = e.payload)),
      // 左鍵點系統匣、頻率上限自動暫停時，同步開關狀態
      listen('monitoring-changed', (e) => (cfg.monitoring = e.payload)),
    ];
    return () => subs.forEach((s) => s.then((off) => off()));
  });
</script>

<div class="shell">
  <nav>
    <div class="brand">Pathie</div>
    {#each pages as p}
      <button class="item" class:active={page === p.id} onclick={() => (page = p.id)}>
        <span class="icon">{@html p.icon}</span>
        <span>{p.label}</span>
      </button>
    {/each}
  </nav>
  <main>
    {#if cfg.ready}
      <current.view />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: flex;
    height: 100vh;
  }
  nav {
    width: 220px;
    flex: none;
    padding: 16px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .brand {
    padding: 4px 12px 16px;
    font-size: 16px;
    font-weight: 600;
  }
  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .item:hover {
    background: var(--sidebar-hover);
  }
  .item.active {
    background: var(--sidebar-active);
  }
  .item.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .icon {
    display: inline-flex;
  }
  .icon :global(svg) {
    width: 20px;
    height: 20px;
    fill: currentColor;
  }
  main {
    flex: 1;
    overflow-y: auto;
    padding: 24px 32px;
  }
</style>
