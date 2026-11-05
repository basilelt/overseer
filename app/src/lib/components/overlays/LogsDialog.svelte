<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Copy from '@lucide/svelte/icons/copy';
  import Save from '@lucide/svelte/icons/save';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Bug from '@lucide/svelte/icons/bug';
  import { feedback, ft } from '../../feedback.svelte';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { isTauri } from '../../client';
  import { diag, diagBackend, dt as tx, fetchServerLog } from '../../diag.svelte';
  import { describeExit, filterLines, formatBundle, iso, parseLines, type DiagInfo, type LevelFilter, type ServerLog } from '../../diag';

  type Tab = 'crash' | 'app' | 'server';

  const previous = $derived(diag.mode === 'previous' ? diag.previous : null);
  const live = $derived(diag.mode === 'live');
  const desktop = isTauri() && !app.mobile;

  let tab = $state<Tab>(diag.mode === 'previous' && diag.previous?.crashReport ? 'crash' : 'app');
  let level = $state<LevelFilter>('all');
  let query = $state('');
  let appLines = $state.raw<string[]>([]);
  let appPath = $state('');
  let server = $state.raw<ServerLog | null>(null);
  let info = $state.raw<DiagInfo | null>(null);
  let pre = $state<HTMLPreElement | null>(null);
  let follow = true;

  const source = $derived(tab === 'server' ? (server?.status === 'ok' ? server.tail.lines : []) : tab === 'crash' ? (previous?.crashReport ?? '').split('\n') : previous ? previous.appLog : appLines);
  const shown = $derived(filterLines(parseLines(source), level, query));
  const serverNote = $derived(
    !server || server.status === 'offline'
      ? tx('offline')
      : server.status === 'unsupported'
        ? tx('unsupported')
        : server.status === 'error'
          ? tx('fetchFailed', { error: server.message })
          : '',
  );
  const levels: LevelFilter[] = ['all', 'info', 'warn', 'error'];
  const tabs = $derived<Tab[]>(previous?.crashReport ? ['crash', 'app', 'server'] : ['app', 'server']);
  const tabKey = { crash: 'tabCrash', app: 'tabApp', server: 'tabServer' } as const;

  async function loadApp() {
    const t = await diagBackend.tail(live ? 2000 : 500).catch(() => null);
    if (!t) return;
    appLines = t.lines;
    appPath = t.path;
  }

  async function loadServer() {
    server = await fetchServerLog(app.client, app.connected, live ? 1000 : 300);
  }

  onMount(() => {
    void diagBackend.info().then((i) => (info = i)).catch(() => {});
    void loadApp();
    if (!live) return;
    const timer = setInterval(() => {
      if (tab === 'app') void loadApp();
      else if (tab === 'server' && app.connected) void loadServer();
    }, 2000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    if (app.connected || server === null) void loadServer();
  });

  $effect(() => {
    void shown;
    if (!pre || !follow) return;
    void tick().then(() => pre && (pre.scrollTop = pre.scrollHeight));
  });

  function onScroll() {
    if (pre) follow = pre.scrollTop + pre.clientHeight >= pre.scrollHeight - 24;
  }

  function bundle(): string {
    return formatBundle({
      generatedAt: Date.now(),
      info,
      previous,
      appLog: appLines,
      server,
      host: app.conn.resolved ?? (app.conn.target.label || app.conn.target.kind),
    });
  }

  async function copyAll() {
    try {
      await app.client?.copyText(bundle());
      app.notify(tx('copied'));
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  async function save() {
    try {
      app.notify(tx('saved', { path: await diagBackend.save(bundle()) }));
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  function openFolder() {
    diagBackend.openDir().catch((e) => app.notify(String((e as Error)?.message ?? e), 'error'));
  }

  const close = () => diag.close();

  function report() {
    if (!previous) return;
    const what = describeExit(previous);
    feedback.show({
      kind: 'bug',
      title: ft('crashTitle', { what }),
      crash: { report: previous.crashReport, appLog: previous.appLog, summary: `Previous session (${previous.version || '?'}) ${what}` },
    });
    close();
  }
</script>

<Modal title={previous ? tx('previousTitle') : tx('title')} onclose={close} width={960}>
  {#if previous}
    <div class="summary">
      <span class="badge {previous.kind}">{tx(previous.kind)}</span>
      <p>{previous.kind === 'freeze' ? tx('freezeHint', { n: previous.frozenSecs }) : tx(previous.kind === 'crash' ? 'crashHint' : 'killedHint')}</p>
      <dl>
        <dt>{tx('started')}</dt>
        <dd class="mono">{iso(previous.startedAt)}</dd>
        <dt>{tx('lastLine')}</dt>
        <dd class="mono">{previous.endedAt ?? '?'}</dd>
        <dt>{tx('version')}</dt>
        <dd class="mono">{previous.version || '?'}</dd>
        {#if previous.view}
          <dt>{tx('view')}</dt>
          <dd class="mono">{previous.view}</dd>
        {/if}
      </dl>
      <p class="faint hint">{tx('hint')}</p>
    </div>
  {/if}

  <div class="bar">
    <div class="seg" role="tablist">
      {#each tabs as t (t)}
        <button role="tab" aria-selected={tab === t} class:on={tab === t} onclick={() => (tab = t)}>{tx(tabKey[t])}</button>
      {/each}
    </div>
    {#if tab !== 'crash'}
      <div class="seg" role="radiogroup" aria-label="Level">
        {#each levels as l (l)}
          <button role="radio" aria-checked={level === l} class:on={level === l} onclick={() => (level = l)}>{tx(l)}</button>
        {/each}
      </div>
      <input class="search" type="search" placeholder={tx('search')} bind:value={query} spellcheck="false" />
    {/if}
    <span class="count faint">{tx('lines', { n: shown.length })}</span>
  </div>

  {#if tab === 'server' && serverNote}
    <p class="note faint">{serverNote}</p>
  {:else}
    <pre class="log" bind:this={pre} onscroll={onScroll}>{#each shown as l, i (i)}<span class="ln {l.level.toLowerCase()}">{l.text}
</span>{:else}<span class="faint">{tx('empty')}</span>{/each}</pre>
  {/if}
  {#if tab === 'app' && !previous && appPath}
    <p class="path faint mono">{appPath}</p>
  {:else if tab === 'server' && server?.status === 'ok'}
    <p class="path faint mono">{server.tail.path}</p>
  {/if}

  {#snippet footer()}
    <button class="btn" onclick={copyAll}><Copy size={14} />{tx('copy')}</button>
    {#if desktop}
      <button class="btn" onclick={save}><Save size={14} />{tx('save')}</button>
      <button class="btn" onclick={openFolder}><FolderOpen size={14} />{tx('openFolder')}</button>
    {/if}
    <span class="spacer"></span>
    {#if previous}
      <button class="btn" onclick={report}><Bug size={14} />{ft('report')}</button>
    {/if}
    <button class="btn primary" onclick={close}>{previous ? tx('dismiss') : tx('close')}</button>
  {/snippet}
</Modal>

<style>
  .summary {
    display: grid;
    gap: 8px;
    padding: 4px 0 12px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 10px;
  }
  .summary p {
    margin: 0;
  }
  .badge {
    justify-self: start;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    border: 1px solid var(--border);
  }
  .badge.crash {
    color: var(--st-failed);
  }
  .badge.freeze {
    color: var(--st-held);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 16px;
    margin: 0;
    font-size: 12px;
  }
  dt {
    color: var(--text-3);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .hint {
    font-size: 11px;
  }
  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 8px;
  }
  .search {
    flex: 1;
    min-width: 120px;
    height: 28px;
  }
  .count {
    font-size: 11px;
    margin-left: auto;
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  .seg button {
    padding: 3px 10px;
    border-radius: 4px;
    color: var(--text-2);
    font-size: 12px;
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
  .log {
    margin: 0;
    height: 46vh;
    overflow: auto;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
    font-family: var(--mono);
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }
  .ln.warn {
    color: var(--st-held);
  }
  .ln.error {
    color: var(--st-failed);
  }
  .ln.debug {
    color: var(--text-3);
  }
  .note {
    margin: 0;
    padding: 24px 0;
    text-align: center;
  }
  .path {
    margin: 6px 0 0;
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: var(--mono);
  }
  .spacer {
    flex: 1;
  }
  :global(.phone) .log {
    height: 52dvh;
  }
  :global(.phone) .search {
    flex-basis: 100%;
  }
</style>
