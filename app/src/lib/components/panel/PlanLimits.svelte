<script lang="ts">
  import { app } from '../../state.svelte';
  import { settings } from '../../settings/settings.svelte';
  import { chipFeed, useChipFeed } from '../../usage/store.svelte';
  import { countdown, formatPct, gaugeLevel, liveWindows, pacePercent } from '../../usage/format';
  import { ust, windowLabel } from '../../usage/strings';
  import LimitBar from '../usage/LimitBar.svelte';

  let { agent }: { agent: string } = $props();

  const session = $derived(app.connected && app.conn.phase === 'connected' && app.client ? `${app.conn.target.kind}:${app.conn.target.host ?? ''}` : null);
  const usage = $derived((settings.usageAgentsHidden ?? []).includes(agent) ? undefined : chipFeed.data?.agents.find((a) => a.agent === agent));
  const rows = $derived.by(() => {
    if (!usage) return [];
    const now = Date.now();
    return liveWindows(usage).map((w) => {
      const pace = pacePercent(w, now);
      const label = windowLabel(w.key);
      const tip = [`${label} ${formatPct(w.usedPercent)}`, pace != null ? ust('pace', { p: formatPct(pace) }) : '', w.resetsAt ? ust('resets', { t: countdown(w.resetsAt, now) ?? '' }) : '', ust('limitRow')];
      return { key: w.key, label, used: w.usedPercent, pace, tone: gaugeLevel(usage, w, now), tip: tip.filter(Boolean).join(' · ') };
    });
  });

  $effect(() => {
    if (!session || !app.client) return;
    return useChipFeed(session, app.client, (fn) => app.onChange((_p, _n, e) => fn(e)), { range: 'today', by: 'agent', claudeLimits: settings.fetchClaudeLimits });
  });
</script>

{#if rows.length}
  <div class="limits" data-tour="plan-limits">
    {#each rows as r (r.key)}
      <button class="row" title={r.tip} aria-label={r.tip} onclick={() => app.open('usage')}>
        <span class="lbl">{r.label}</span>
        <LimitBar used={r.used} pace={r.pace} tone={r.tone} />
        <span class="pct mono {r.tone}">{formatPct(r.used)}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .limits {
    --use-warn: #b45309;
    display: flex;
    flex-direction: column;
    margin: 6px -6px 0;
  }
  :global(:root[data-theme='dark']) .limits {
    --use-warn: #f59e0b;
  }
  .row {
    display: grid;
    grid-template-columns: 4.5em minmax(40px, 1fr) 3em;
    align-items: center;
    gap: 8px;
    padding: 3px 6px;
    border-radius: var(--radius);
    font-size: 11px;
    color: var(--text-2);
    text-align: left;
    transition: background var(--ease);
  }
  .row:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .lbl {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pct {
    text-align: right;
  }
  .pct.warn {
    color: var(--use-warn);
  }
  .pct.crit {
    color: var(--st-failed);
  }
  :global(:root.phone) .row {
    padding: 8px 6px;
    font-size: 13px;
  }
</style>
