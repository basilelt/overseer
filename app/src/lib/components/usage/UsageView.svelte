<script lang="ts">
  import { app } from '../../state.svelte';
  import { settings } from '../../settings.svelte';
  import { t, getLocale } from '../../i18n.svelte';
  import { agentName } from '../../agents';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import Bars from './Bars.svelte';
  import TaskCosts from './TaskCosts.svelte';
  import { UsageFeed } from '../../usage/store.svelte';
  import { agentLevel, authNote, countdown, formatCost, formatDuration, formatPct, formatTokens, liveWindows, onCredits, shortDay, windowLevel } from '../../usage/format';
  import { creditsNote, ust } from '../../usage/strings';
  import type { UsageBy, UsageGroup, UsageRange } from '../../usage/types';

  let { compact = false }: { compact?: boolean } = $props();

  const feed = new UsageFeed();
  let range = $state<UsageRange>('today');
  let by = $state<UsageBy>('project');
  let now = $state(Date.now());

  const session = $derived(app.connected && app.conn.phase === 'connected' && app.client ? `${app.conn.target.kind}:${app.conn.target.host ?? ''}` : null);
  const host = $derived(app.conn.target.kind === 'local' ? t('host.local') : app.conn.target.label);
  const data = $derived(feed.data);
  const totals = $derived(data?.totals);
  const groups = $derived(data?.groups ?? []);
  const maxGroup = $derived(Math.max(1, ...groups.map((g) => g.usage.total)));
  const locale = $derived(getLocale());
  const ranges: { id: UsageRange; label: () => string }[] = [
    { id: 'today', label: () => ust('today') },
    { id: '7d', label: () => ust('d7') },
    { id: '30d', label: () => ust('d30') },
  ];
  const bys: { id: UsageBy; label: () => string }[] = [
    { id: 'project', label: () => ust('byProject') },
    { id: 'session', label: () => ust('bySession') },
    { id: 'model', label: () => ust('byModel') },
    { id: 'agent', label: () => ust('byAgent') },
  ];

  $effect(() => {
    if (!session || !app.client) return;
    return feed.use(session, app.client, (fn) => app.onChange((_p, _n, e) => fn(e)), { range, by, claudeLimits: settings.fetchClaudeLimits });
  });

  $effect(() => {
    if (!feed.visible || !(data?.agents ?? []).some((a) => liveWindows(a).length > 0)) return;
    now = Date.now();
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });

  function groupLabel(g: UsageGroup): string {
    if (by === 'agent') return agentName(app.agents, g.agent ?? g.key);
    if (by === 'session') return g.title || ust('untitled', { id: (g.sessionId ?? g.key).slice(0, 8) });
    if (by === 'model') return g.model ?? g.key;
    return g.project ?? g.key;
  }

  function openSession(g: UsageGroup) {
    if (!g.lineId) return;
    const key = `line:${g.lineId}`;
    if (!app.items.some((i) => i.key === key)) return;
    app.open(null);
    app.select(key);
  }

  function ago(at: string | undefined): string {
    const t0 = at ? Date.parse(at) : NaN;
    return Number.isFinite(t0) ? formatDuration(now - t0) : '';
  }
</script>

<div class="use" class:compact>
  <div class="head">
    <span class="host">{host}</span>
    <div class="seg" role="tablist">
      {#each ranges as r (r.id)}
        <button role="tab" aria-selected={range === r.id} class:on={range === r.id} onclick={() => (range = r.id)}>{r.label()}</button>
      {/each}
    </div>
    {#if data?.indexing}<span class="faint small">{ust('indexing')}</span>{/if}
    {#if !feed.visible}<span class="faint small paused">{ust('paused')}</span>{/if}
  </div>

  {#if feed.unavailable}
    <p class="empty">{ust('unavailable')}</p>
  {:else if !session}
    <p class="empty">{t('conn.idle')}</p>
  {:else if !data}
    <p class="empty">{ust('indexing')}</p>
  {:else}
    {#if data.agents.length}
      <section class="block">
        <h3>{ust('limits')}</h3>
        <div class="limits">
          {#each data.agents as a (a.agent)}
            {@const windows = a.limits.windows}
            <div class="agent card {agentLevel(a)}">
              <header>
                <span class="who"><AgentGlyph agent={a.agent} size={13} />{agentName(app.agents, a.agent)}</span>
                <span class="faint mono small">{formatTokens(a.today.total)} · {formatCost(a.today.cost, a.today.costPartial)}</span>
              </header>
              {#if a.limits.hit}
                <p class="hit">{ust('limitHitLong')}{a.limits.hit.message ? ` · ${a.limits.hit.message}` : ''}</p>
              {/if}
              {#each windows as w (w.key)}
                {@const tone = windowLevel(a, w)}
                <div class="gauge" title={w.resetsAt ? ust('resetAt', { at: new Date(w.resetsAt).toLocaleString(locale) }) : ''}>
                  <span class="wkey mono">{w.key}</span>
                  <span class="bar"><span class="fill {tone}" style:width="{Math.min(100, w.usedPercent)}%"></span></span>
                  <span class="pct mono {tone}">{formatPct(w.usedPercent)}</span>
                  <span class="when faint small">{w.stale ? ust('wasReset') : w.resetsAt ? ust('resets', { t: countdown(w.resetsAt, now) ?? '' }) : ''}</span>
                </div>
              {/each}
              {#if onCredits(a)}
                <p class="credits small">{creditsNote(a)}</p>
              {/if}
              {#if authNote(a.limits)}
                <p class="auth small">{ust(authNote(a.limits)!)}</p>
              {/if}
              {#if !a.limits.source && !a.limits.hit && !authNote(a.limits)}
                <p class="faint small">{a.agent === 'claude' ? ust('noClaudeLimits') : ust('noLimits', { agent: agentName(app.agents, a.agent) })}</p>
              {:else if a.limits.observedAt}
                <p class="faint tiny">{a.limits.plan ? `${a.limits.plan} · ` : ''}{ust('observed', { t: ago(a.limits.observedAt) })}{a.limits.via === 'statusline' ? ` · ${ust('viaStatusline')}` : ''}</p>
              {/if}
            </div>
          {/each}
        </div>
      </section>
    {/if}

    {#if totals}
      <div class="stats">
        <div class="stat">
          <span class="k">{ust('tokens')}</span>
          <span class="v mono">{formatTokens(totals.total)}</span>
          <span class="s faint mono">{ust('input')} {formatTokens(totals.input)} · {ust('output')} {formatTokens(totals.output)} · {ust('cache')} {formatTokens(totals.cacheRead + totals.cacheWrite)}</span>
        </div>
        <div class="stat" title={ust('costHint')}>
          <span class="k">{ust('cost')}</span>
          <span class="v mono">{formatCost(totals.cost, totals.costPartial)}</span>
          <span class="s faint">{ust('costShort')}</span>
        </div>
        <div class="stat">
          <span class="k">{ust('messages')}</span>
          <span class="v mono">{totals.messages.toLocaleString(locale)}</span>
        </div>
        <div class="stat">
          <span class="k">{ust('active')}</span>
          <span class="v mono">{formatDuration(totals.activeMs)}</span>
        </div>
      </div>
    {/if}

    {#if range !== 'today' && data.days.length > 1}
      <section class="block">
        <h3>{ust('perDay')}</h3>
        <Bars
          values={data.days.map((d) => d.usage.total)}
          labels={data.days.map((d) => shortDay(d.day, locale))}
          titles={data.days.map((d) => `${shortDay(d.day, locale)} · ${formatTokens(d.usage.total)} · ${formatCost(d.usage.cost, d.usage.costPartial)}`)}
          label={ust('perDay')}
        />
      </section>
    {/if}

    <section class="block">
      <div class="h3row">
        <h3>{ust('breakdown')}</h3>
        <div class="seg" role="tablist">
          {#each bys as b (b.id)}
            <button role="tab" aria-selected={by === b.id} class:on={by === b.id} onclick={() => (by = b.id)}>{b.label()}</button>
          {/each}
        </div>
      </div>
      {#if !groups.length}
        <p class="faint small">{ust('empty')}</p>
      {:else}
        <table>
          <tbody>
            {#each groups as g (g.key)}
              <tr class:click={!!g.lineId} onclick={() => openSession(g)}>
                <td class="name">
                  {#if g.agent && by !== 'agent'}<AgentGlyph agent={g.agent} size={11} />{/if}
                  <span class="label" title={g.key}>{groupLabel(g)}</span>
                  {#if by === 'session' && g.project}<span class="faint small"> · {g.project}</span>{/if}
                </td>
                <td class="barcell wide"><span class="bar"><span class="fill" style:width="{(g.usage.total / maxGroup) * 100}%"></span></span></td>
                <td class="num mono">{formatTokens(g.usage.total)}</td>
                <td class="num mono faint">{formatCost(g.usage.cost, g.usage.costPartial)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
    {#if !compact}<TaskCosts />{/if}
  {/if}
</div>

<style>
  .use {
    --use-warn: #b45309;
    display: flex;
    flex-direction: column;
    gap: 14px;
    font-size: 13px;
    min-width: 0;
  }
  :global(:root[data-theme='dark']) .use {
    --use-warn: #f59e0b;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
  }
  .host {
    font-weight: 600;
  }
  .paused {
    margin-left: auto;
  }
  .empty {
    color: var(--text-2);
    margin: 24px 0;
    text-align: center;
  }
  .small {
    font-size: 12px;
    margin: 0;
  }
  .tiny {
    font-size: 11px;
    margin: 0;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .seg button {
    padding: 2px 8px;
    font-size: 11px;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--surface-active);
    color: var(--text);
  }
  .block h3 {
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  .h3row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 6px;
  }
  .h3row h3 {
    margin: 0;
  }
  .limits {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 10px;
  }
  .card {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .card.warn {
    border-color: color-mix(in srgb, var(--use-warn) 45%, var(--border));
  }
  .card.crit {
    border-color: color-mix(in srgb, var(--st-failed) 45%, var(--border));
  }
  .card header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }
  .who {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 500;
  }
  .auth,
  .credits {
    margin: 4px 0 0;
    color: var(--text-2);
  }
  .hit {
    margin: 0;
    color: var(--st-failed);
    font-size: 12px;
  }
  .gauge {
    display: grid;
    grid-template-columns: 28px minmax(60px, 1fr) 40px 8.5em;
    align-items: center;
    gap: 8px;
  }
  .wkey {
    color: var(--text-2);
    font-size: 12px;
  }
  .bar {
    display: block;
    height: 6px;
    background: var(--surface-2);
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--text-3);
  }
  .fill.warn {
    background: var(--use-warn);
  }
  .fill.crit {
    background: var(--st-failed);
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
  .when {
    white-space: nowrap;
    text-align: right;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 10px;
  }
  .stat {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .stat .k {
    color: var(--text-2);
    font-size: 12px;
  }
  .stat .v {
    font-size: 18px;
    font-weight: 500;
  }
  .stat .s {
    font-size: 11px;
    line-height: 1.35;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }
  td {
    padding: 4px 0;
    border-top: 1px solid var(--border);
  }
  td.name {
    width: 46%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  td.name :global(svg) {
    vertical-align: -1px;
    margin-right: 4px;
  }
  td.barcell {
    padding: 0 10px;
  }
  .num {
    text-align: right;
    white-space: nowrap;
    width: 72px;
  }
  tr.click {
    cursor: pointer;
  }
  tr.click:hover td {
    background: var(--surface-hover);
  }
  .compact .wide {
    display: none;
  }
  .compact td.name {
    width: 60%;
  }
</style>
