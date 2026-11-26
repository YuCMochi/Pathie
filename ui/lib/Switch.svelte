<script>
  let { checked = false, disabled = false, onchange, label = '' } = $props();
</script>

<button
  class="switch"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange?.(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    position: relative;
    width: 40px;
    height: 20px;
    border-radius: 10px;
    border: 1px solid var(--switch-off);
    background: transparent;
    padding: 0;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
    flex: none;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--switch-off);
    transition: transform 0.15s, background 0.15s, width 0.15s;
  }
  .switch:hover .knob {
    width: 14px;
    height: 14px;
    top: 2px;
  }
  .on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .on .knob {
    background: var(--accent-text);
    transform: translateX(20px);
  }
  .on:hover .knob {
    transform: translateX(19px);
  }
  .switch:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .switch:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
