<script>
  // options: [{ value, label }]
  let { value, options, onchange, label = '' } = $props();
</script>

<select
  aria-label={label}
  {value}
  onchange={(e) => onchange?.(options.find((o) => String(o.value) === e.currentTarget.value).value)}
>
  {#each options as o}
    <option value={o.value}>{o.label}</option>
  {/each}
</select>

<style>
  select {
    min-width: 180px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-bottom-color: var(--switch-off);
    border-radius: 4px;
    background: var(--surface-hover);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  select:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
