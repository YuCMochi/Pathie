<script>
  let { title, desc = '', disabled = false, children } = $props();
</script>

<div class="row" class:disabled>
  <div class="text">
    <div class="title">{title}</div>
    {#if desc}<div class="desc">{desc}</div>{/if}
  </div>
  <div class="control">{@render children?.()}</div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 56px;
    padding: 12px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    margin-bottom: 4px;
  }
  .row.disabled .text {
    opacity: 0.5;
  }
  .title {
    font-size: 14px;
  }
  .desc {
    margin-top: 2px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-dim);
  }
  .control {
    flex: none;
  }
</style>
