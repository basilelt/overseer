<script lang="ts">
  import { app } from '../../state.svelte';
</script>

{#if app.toast}
  {@const action = app.toast.action}
  {@const progress = app.toast.progress}
  <div class="toast" class:error={app.toast.kind === 'error'} role="status">
    <span class="text">{app.toast.text}</span>
    {#if progress !== undefined}
      <span class="pct">{Math.round(progress * 100)}%</span>
      <span class="bar" style:width="{progress * 100}%"></span>
    {/if}
    {#if action}
      <button class="btn" onclick={() => app.runToastAction()}>{action.label}</button>
    {/if}
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
    border-radius: var(--radius);
    padding: 8px 14px;
    z-index: 100;
    width: max-content;
    max-width: min(480px, calc(100vw - 32px));
    display: flex;
    align-items: center;
    gap: 12px;
    overflow: hidden;
  }
  .text {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .pct {
    flex: none;
    color: var(--text-3);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    background: var(--accent);
    transition: width var(--ease);
  }
  .btn {
    flex: none;
    height: 24px;
    font-size: 12px;
    white-space: nowrap;
  }
  :global(:root.phone) .toast {
    bottom: calc(72px + var(--safe-bottom) + var(--kb, 0px) + var(--composer-h, 0px) + 8px);
  }
  :global(:root.phone:has([role='menu'])) .toast {
    bottom: auto;
    top: calc(8px + var(--safe-top));
  }
  .error {
    color: var(--st-failed);
  }
</style>
