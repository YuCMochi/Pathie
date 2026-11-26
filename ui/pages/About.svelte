<script>
  import { cfg, openRepository } from '../lib/store.svelte.js';
</script>

<h1>關於</h1>
<p><b>Pathie</b> v{cfg.info.version}</p>
<p>作者：{cfg.info.author}</p>
<p>
  <button class="link" onclick={openRepository}>{cfg.info.repository}</button>
</p>

<style>
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
  }
</style>
