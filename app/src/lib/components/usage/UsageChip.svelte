<script lang="ts">
  import Gauge from '@lucide/svelte/icons/gauge';
  import { app } from '../../state.svelte';
  import { settings } from '../../settings/settings.svelte';
  import { agentName } from '../../agents';
  import { chipFeed } from '../../usage/store.svelte';
  import { authNote, chipSegments, countdown, onCredits, worst } from '../../usage/format';
  import { creditsNote, ust } from '../../usage/strings';

  const session = $derived(app.connected && app.conn.phase === 'connected' && app.client ? `${app.conn.target.kind}:${app.conn.target.host ?? ''}` : null);
  const segs = $derived(chipFeed.data && !chipFeed.unavailable ? chipSegments(chipFeed.data.agents, (id) => agentName(app.agents, id), ust('limitHit'), ust('credits')) : []);
  const tone = $derived(worst(segs.map((s) => s.level)));
  const tip = $derived.by(() => {
    const now = Date.now();
    const lines = (chipFeed.data?.agents ?? []).flatMap((a) =>
      a.limits.windows
        .filter((w) => !w.stale && w.resetsAt)
        .map((w) => `${agentName(app.agents, a.agent)} ${w.key} ${Math.round(w.usedPercent)}% · ${ust('resets', { t: countdown(w.resetsAt, now) ?? '' })}`),
    );
    const hits = (chipFeed.data?.agents ?? []).filter((a) => a.limits.hit).map((a) => `${agentName(app.agents, a.agent)}: ${a.limits.hit!.message || ust('limitHitLong')}`);
    const auth = (chipFeed.data?.agents ?? []).filter((a) => authNote(a.limits)).map((a) => `${agentName(app.agents, a.agent)}: ${ust(authNote(a.limits)!)}`);
    const credits = (chipFeed.data?.agents ?? []).filter(onCredits).map((a) => `${agentName(app.agents, a.agent)}: ${creditsNote(a)}`);
    return [ust('chip'), ...hits, ...lines, ...credits, ...auth].join('\n');
  });

  $effect(() => {
    if (!session || !app.client) return;
    return chipFeed.use(session, app.client, (fn) => app.onChange((_p, _n, e) => fn(e)), { range: 'today', by: 'agent', claudeLimits: settings.fetchClaudeLimits });
  });
</script>

{#if segs.length}
  <button class="chip {tone}" data-tour="chips" title={tip} aria-label={tip} onclick={() => app.open('usage')}>
    <Gauge size={13} />
    {#each segs as s, i (s.agent)}
      {#if i > 0}<span class="sep" aria-hidden="true"></span>{/if}
      <span class="seg {s.level}"><span class="name">{s.name}</span> <span class="mono">{s.parts.join(' · ')}</span></span>
    {/each}
  </button>
{/if}

<style>
  .chip {
    --use-warn: #b45309;
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
    max-width: 440px;
    min-width: 0;
    overflow: hidden;
    transition: background var(--ease);
  }
  :global(:root[data-theme='dark']) .chip {
    --use-warn: #f59e0b;
  }
  .chip > :global(svg) {
    flex-shrink: 0;
  }
  .chip:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .chip.warn {
    border-color: color-mix(in srgb, var(--use-warn) 45%, var(--border));
  }
  .chip.crit {
    border-color: color-mix(in srgb, var(--st-failed) 45%, var(--border));
  }
  .seg {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .seg.warn {
    color: var(--use-warn);
  }
  .seg.crit {
    color: var(--st-failed);
  }
  .name {
    font-weight: 500;
  }
  .sep {
    width: 1px;
    height: 12px;
    background: var(--border);
    margin: 0 2px;
  }
</style>
