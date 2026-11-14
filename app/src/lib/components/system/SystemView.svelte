<script lang="ts">
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { system } from '../../system/monitor.svelte';
  import { systemSession } from '../../system/session';
  import { st } from '../../system/strings';
  import { agentLabel, cpuPct, formatBytes, formatPct, formatRate, formatUptime, memPct, niceMax } from '../../system/format';
  import { level, type Level } from '../../severity';
  import type { SysProc } from '../../system/types';
  import Spark from './Spark.svelte';

  let { compact = false }: { compact?: boolean } = $props();

  const session = $derived(systemSession(app.connected, app.conn, !!app.client));
  const host = $derived(app.conn.target.kind === 'local' ? st('local') : app.conn.target.label);
  const cur = $derived(system.current);
  const hist = $derived(system.history);
  const times = $derived(hist.map((p) => p.t));
  const t1 = $derived(cur?.t ?? (times.length ? times[times.length - 1] : 0));
  const gapMs = $derived(Math.max(6000, system.intervalMs * 3));
  const cpu = $derived(cpuPct(cur));
  const mem = $derived(memPct(cur));
  const memTotal = $derived(cur?.mem?.total ?? (hist.length ? hist[hist.length - 1].memTotal : 0));
  const netMax = $derived(niceMax(Math.max(1024, ...hist.map((p) => Math.max(p.rx, p.tx)))));
  const diskMax = $derived(niceMax(Math.max(1024, ...hist.map((p) => Math.max(p.read, p.write)))));
  const rx = $derived((cur?.net ?? []).reduce((a, n) => a + n.rx, 0));
  const tx = $derived((cur?.net ?? []).reduce((a, n) => a + n.tx, 0));
  const rd = $derived((cur?.disks ?? []).reduce((a, d) => a + d.read, 0));
  const wr = $derived((cur?.disks ?? []).reduce((a, d) => a + d.write, 0));
  let sort = $state<'cpu' | 'mem'>('cpu');
  const procs = $derived<SysProc[]>((sort === 'cpu' ? cur?.top?.cpu : cur?.top?.mem) ?? []);

  $effect(() => {
    if (!session || !app.client) return;
    return system.use(session, app.client, 'live');
  });

  function fsPct(used: number, total: number): number {
    return total > 0 ? (used / total) * 100 : 0;
  }

  function openAgent(termId: string, lineId: string | null) {
    const key = [lineId ? `line:${lineId}` : null, `term:${termId}`].find((k) => k && app.items.some((i) => i.key === k));
    if (key) {
      app.open(null);
      app.select(key);
    }
  }

  const tone = (n: number | null): Level => level(n);
</script>

<div class="sys" class:compact>
  <div class="head">
    <span class="host">{host}</span>
    {#if cur?.supported}
      <span class="faint">{cur.platform}</span>
      {#if cur.cpu?.cores?.length}<span class="faint">{st('cores', { n: cur.cpu.cores.length })}</span>{/if}
      {#if cur.load}<span class="faint">{st('load')} <span class="mono">{cur.load.map((l) => l.toFixed(2)).join(' ')}</span></span>{/if}
      {#if cur.uptimeSec != null}<span class="faint">{st('uptime')} <span class="mono">{formatUptime(cur.uptimeSec)}</span></span>{/if}
    {/if}
    {#if !system.visible}<span class="faint paused">{st('paused')}</span>{/if}
  </div>

  {#if system.unavailable}
    <p class="empty">{st('unavailable')}</p>
  {:else if !session}
    <p class="empty">{t('conn.idle')}</p>
  {:else if cur && !cur.supported}
    <p class="empty">{st('unsupported', { platform: cur.platform })}</p>
  {:else if !cur}
    <p class="empty">{st('waiting')}</p>
  {:else}
    <div class="cards">
      <section class="card">
        <header><span>{st('cpu')}</span><span class="val mono {tone(cpu)}">{formatPct(cpu)}</span></header>
        <Spark {times} a={hist.map((p) => p.cpu)} max={100} {t1} {gapMs} tone={tone(cpu)} label={st('cpu')} />
        {#if cur.cpu?.cores?.length}
          <div class="cores" aria-hidden="true">
            {#each cur.cpu.cores as c, i (i)}<span class="core {tone(c)}" style:--v="{Math.min(100, c)}%" title="{i}: {formatPct(c)}"></span>{/each}
          </div>
        {/if}
        {#if cur.pressure}
          <div class="sub faint">{st('pressure')} <span class="mono">cpu {cur.pressure.cpu ?? '—'} · mem {cur.pressure.memory ?? '—'} · io {cur.pressure.io ?? '—'}</span></div>
        {/if}
      </section>
      <section class="card">
        <header><span>{st('memory')}</span><span class="val mono {tone(mem)}">{formatPct(mem)}</span></header>
        <Spark {times} a={hist.map((p) => p.memUsed)} b={hist.map((p) => p.swapUsed)} max={memTotal || 1} {t1} {gapMs} tone={tone(mem)} label={st('memory')} />
        <div class="sub faint mono">
          {formatBytes(cur.mem?.used)} / {formatBytes(cur.mem?.total)} · {st('cached')} {formatBytes(cur.mem?.cached)}{#if cur.mem?.swapTotal}
            · {st('swap')} {formatBytes(cur.mem.swapUsed)} / {formatBytes(cur.mem.swapTotal)}{/if}
        </div>
      </section>
      <section class="card">
        <header><span>{st('network')}</span><span class="val mono">↓ {formatRate(rx)} ↑ {formatRate(tx)}</span></header>
        <Spark {times} a={hist.map((p) => p.rx)} b={hist.map((p) => p.tx)} max={netMax} {t1} {gapMs} label={st('network')} />
        <div class="sub faint mono">
          {#each cur.net ?? [] as n (n.name)}<span>{n.name} ↓ {formatRate(n.rx)} ↑ {formatRate(n.tx)}</span>{/each}
        </div>
      </section>
      <section class="card">
        <header><span>{st('disk')}</span><span class="val mono">R {formatRate(rd)} W {formatRate(wr)}</span></header>
        <Spark {times} a={hist.map((p) => p.read)} b={hist.map((p) => p.write)} max={diskMax} {t1} {gapMs} label={st('disk')} />
        <div class="sub faint mono">
          {#each (cur.disks ?? []).filter((d) => d.read + d.write > 0).slice(0, 6) as d (d.name)}<span>{d.name} R {formatRate(d.read)} W {formatRate(d.write)}</span>{/each}
        </div>
      </section>
    </div>

    {#if cur.fs?.length}
      <section class="block">
        <h3>{st('filesystems')}</h3>
        {#each cur.fs as f (f.mount + f.device)}
          {@const p = fsPct(f.used, f.total)}
          <div class="fs" title={f.device}>
            <span class="name mono">{f.mount}</span>
            <span class="bar"><span class="fill {tone(p)}" style:width="{p}%"></span></span>
            <span class="num mono {tone(p)}">{formatPct(p)}</span>
            <span class="num faint mono wide">{st('free', { free: formatBytes(f.avail), total: formatBytes(f.total) })}</span>
          </div>
        {/each}
      </section>
    {/if}

    <section class="block">
      <h3>{st('agents')}</h3>
      {#if cur.agents?.length}
        <table>
          <tbody>
            {#each cur.agents as a (a.termId)}
              <tr class="click" onclick={() => openAgent(a.termId, a.lineId)}>
                <td class="name">{agentLabel(a, st('in'))}</td>
                <td class="num mono">{formatPct(a.cpu)}</td>
                <td class="num mono">{formatBytes(a.rss)}</td>
                <td class="num faint mono wide">{st('procs', { n: a.procs })}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="faint small">{st('noAgents')}</p>
      {/if}
    </section>

    <section class="block">
      <div class="h3row">
        <h3>{st('processes')}</h3>
        <div class="seg" role="tablist">
          <button role="tab" aria-selected={sort === 'cpu'} class:on={sort === 'cpu'} onclick={() => (sort = 'cpu')}>{st('byCpu')}</button>
          <button role="tab" aria-selected={sort === 'mem'} class:on={sort === 'mem'} onclick={() => (sort = 'mem')}>{st('byMem')}</button>
        </div>
      </div>
      <table>
        <tbody>
          {#each procs as p (p.pid)}
            <tr>
              <td class="name">{p.name}</td>
              <td class="num faint mono wide">{p.pid}</td>
              <td class="num mono">{formatPct(p.cpu)}</td>
              <td class="num mono">{formatBytes(p.rss)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</div>

<style>
  .sys {
    --sys-warn: #b45309;
    display: flex;
    flex-direction: column;
    gap: 14px;
    font-size: 13px;
    min-width: 0;
  }
  :global(:root[data-theme='dark']) .sys {
    --sys-warn: #f59e0b;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 12px;
  }
  .host {
    font-weight: 600;
  }
  .paused {
    margin-left: auto;
    font-size: 12px;
  }
  .empty {
    color: var(--text-2);
    margin: 24px 0;
    text-align: center;
  }
  .cards {
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
  .card header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    color: var(--text-2);
    font-size: 12px;
  }
  .val {
    color: var(--text);
    font-size: 13px;
    white-space: nowrap;
  }
  .sub {
    font-size: 11px;
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
  }
  .cores {
    display: flex;
    gap: 2px;
    height: 14px;
    align-items: flex-end;
  }
  .core {
    flex: 1;
    min-width: 2px;
    height: 100%;
    background: linear-gradient(to top, var(--c, var(--text-3)) var(--v), var(--surface-2) var(--v));
    border-radius: 1px;
  }
  .warn {
    --c: var(--sys-warn);
    color: var(--sys-warn);
  }
  .crit {
    --c: var(--st-failed);
    color: var(--st-failed);
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
    margin-bottom: 6px;
  }
  .h3row h3 {
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
  .fs {
    display: grid;
    grid-template-columns: minmax(80px, 1.2fr) 2fr 44px minmax(0, 1.4fr);
    align-items: center;
    gap: 10px;
    padding: 3px 0;
  }
  .bar {
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
    background: var(--sys-warn);
  }
  .fill.crit {
    background: var(--st-failed);
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
  td.name,
  .fs .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  td.name {
    width: 50%;
  }
  .num {
    text-align: right;
    white-space: nowrap;
  }
  tr.click {
    cursor: pointer;
  }
  tr.click:hover td {
    background: var(--surface-hover);
  }
  .small {
    font-size: 12px;
    margin: 0;
  }
  .compact .wide {
    display: none;
  }
  .compact .fs {
    grid-template-columns: minmax(80px, 1fr) 1.5fr 44px;
  }
</style>
