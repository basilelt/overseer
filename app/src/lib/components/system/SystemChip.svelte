<script lang="ts">
  import Activity from '@lucide/svelte/icons/activity';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { system } from '../../system/monitor.svelte';
  import { chipLevel, chipText } from '../../system/format';
  import { systemSession } from '../../system/session';
  import { st } from '../../system/strings';

  const session = $derived(systemSession(app.connected, app.conn, !!app.client));
  const text = $derived(system.unavailable ? null : chipText(system.current));
  const tone = $derived(chipLevel(system.current));
  const host = $derived(app.conn.target.kind === 'local' ? t('host.local') : app.conn.target.label);

  $effect(() => {
    if (!session || !app.client) return;
    return system.use(session, app.client, 'chip');
  });
</script>

{#if text}
  <button class="chip {tone}" data-tour="chips" title={st('chip', { host })} aria-label={st('chip', { host })} onclick={() => app.open('system')}>
    <Activity size={13} />
    <span class="mono">{text}</span>
  </button>
{/if}

<style>
  .chip {
    --sys-warn: #b45309;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 13px;
    color: var(--text-2);
    font-size: 12px;
    white-space: nowrap;
    transition: background var(--ease);
  }
  :global(:root[data-theme='dark']) .chip {
    --sys-warn: #f59e0b;
  }
  .chip:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .chip.warn {
    color: var(--sys-warn, #d97706);
    border-color: color-mix(in srgb, var(--sys-warn, #d97706) 45%, var(--border));
  }
  .chip.crit {
    color: var(--st-failed);
    border-color: color-mix(in srgb, var(--st-failed) 45%, var(--border));
  }
</style>
