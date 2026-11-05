<script lang="ts">
  import LogOut from '@lucide/svelte/icons/log-out';
  import { exitDemo, isDemo } from '../../demo';
  import { t } from '../../i18n.svelte';

  let { compact = false }: { compact?: boolean } = $props();

  const demo = isDemo();
</script>

{#if demo}
  <div class="demo" class:compact data-tour="demo" role="status" title={t('demo.exitHint')}>
    <span class="dot"></span>
    <span class="label">{t(compact ? 'demo.short' : 'demo.badge')}</span>
    <button class="exit" aria-label={t('demo.exit')} title={t('demo.exit')} onclick={() => exitDemo()}>
      {#if compact}<LogOut size={14} />{:else}{t('demo.exit')}{/if}
    </button>
  </div>
{/if}

<style>
  .demo {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    height: 22px;
    padding: 0 2px 0 9px;
    border: 1px solid var(--accent);
    border-radius: 11px;
    background: var(--accent-soft);
    color: var(--text);
    font-size: 11.5px;
    font-weight: 500;
    white-space: nowrap;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .exit {
    height: 18px;
    padding: 0 8px;
    border: 0;
    border-radius: 9px;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  .exit:hover {
    background: var(--bg-subtle);
  }
  .compact {
    height: 36px;
    padding: 0 0 0 10px;
    border-radius: 18px;
    font-size: 12px;
  }
  .compact .exit {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: 17px;
    background: transparent;
  }
</style>
