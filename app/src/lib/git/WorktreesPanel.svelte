<script lang="ts">
  import { gesture } from '../links';
  import { onMount } from 'svelte';
  import BrushCleaning from '@lucide/svelte/icons/brush-cleaning';
  import Eye from '@lucide/svelte/icons/eye';
  import GitMerge from '@lucide/svelte/icons/git-merge';
  import Plus from '@lucide/svelte/icons/plus';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import AgentGlyph from '../components/ui/AgentGlyph.svelte';
  import { app } from '../state.svelte';
  import { age } from '../time';
  import { gitApi } from './api';
  import CleanupDialog from './CleanupDialog.svelte';
  import { gt, type GKey } from './strings';
  import { openReview } from '../review/open';
  import type { Worktree, WorktreeList } from './types';
  import { cleanable, formatSize, listFor, sortWorktrees, unused } from './view';

  let { projectPath }: { projectPath: string } = $props();

  let data = $state<WorktreeList | null>(null);
  let error = $state<string | null>(null);
  let cleaning = $state<Worktree[] | null>(null);
  let busy = $state<string | null>(null);

  const list = $derived(data ? sortWorktrees(data.worktrees) : []);
  const stale = $derived(list.filter(cleanable));
  const idle = $derived(list.filter(unused));
  const err = (e: unknown) => String((e as Error)?.message ?? e);

  async function load() {
    if (!app.client) return;
    try {
      const d = await gitApi(app.client).worktrees(projectPath);
      if (listFor(d.project, projectPath)) data = d;
      error = null;
    } catch (e) {
      error = err(e);
    }
  }

  $effect(() => {
    projectPath;
    data = null;
    load();
  });

  onMount(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const off = app.onChange((_p, _n, e) => {
      if (!/^(git|worktree)\./.test(e.type)) return;
      const p = e.data?.project ?? e.data?.worktree?.project;
      if (p && p !== projectPath) return;
      clearTimeout(timer);
      timer = setTimeout(load, 250);
    });
    return () => {
      off();
      clearTimeout(timer);
    };
  });

  async function act(w: Worktree, fn: () => Promise<unknown>, ok?: string) {
    busy = w.id;
    try {
      await fn();
      if (ok) app.notify(ok);
      await load();
    } catch (e) {
      app.notify(err(e), 'error');
    } finally {
      busy = null;
    }
  }

  async function openTab(w: Worktree) {
    if (w.termIds[0]) return app.select(`term:${w.termIds[0]}`);
    if (w.lines[0]) return app.select(`line:${w.lines[0].id}`);
    try {
      await app.newSession('shell', { cwd: w.path, worktree: false });
    } catch (e) {
      app.notify(err(e), 'error');
    }
  }

  function review(w: Worktree) {
    openReview({ worktree: w.path });
  }

  const canReview = (w: Worktree) => w.state !== 'missing';
  const newWorktree = () =>
    act({ id: '' } as Worktree, async () => {
      const w = await gitApi(app.requireClient()).createWorktree({ project: data?.project ?? projectPath });
      app.notify(gt('worktreeCreated', { branch: w.branch ?? w.path }));
    });
  const shortPath = (p: string) => (data?.store && p.startsWith(`${data.store}/`) ? p.slice(data.store.length + 1) : p);
  const lineState = (l: Worktree['lines'][number]) => (l.needsInput ? 'input' : l.busy ? 'working' : l.state === 'held' ? 'held' : 'idle');
</script>

{#snippet row(w: Worktree, readonly: boolean)}
  <li class="wt {w.state}" class:main={w.kind === 'main'}>
    <span class="state" title={gt(`state.${w.state}` as GKey)}><span class="dot"></span>{gt(`state.${w.state}` as GKey)}</span>
    <div class="who">
      <div class="top">
        <span class="mono branch">{w.branch ?? w.head.slice(0, 7)}</span>
        {#if w.kind === 'main'}<span class="faint small">{gt('main')}</span>{/if}
        {#if w.ticketKey}<span class="mono faint small">{w.ticketKey}</span>{/if}
        {#if w.pr}<button class="pr {w.pr.state.toLowerCase()}" onclick={(e) => app.openUrl(w.pr!.url, undefined, gesture(e))}>{gt('pr', { n: w.pr.number })}</button>{/if}
      </div>
      <div class="sub">
        {#each w.lines as l (l.id)}
          <span class="line"><AgentGlyph agent={l.agent} size={11} /><span class="st {lineState(l)}"></span>{l.agent}</span>
        {/each}
        {#if w.task}<span class="task">{gt('task', { n: w.task.number })} · {w.task.title}</span>{/if}
        <span class="mono faint path" title={w.path}>{shortPath(w.path)}</span>
      </div>
    </div>
    <span class="sync mono">
      {#if w.state !== 'missing' && (w.kind !== 'main' || w.ahead || w.behind)}
        <span class:faint={!w.ahead}>↑{w.ahead ?? 0}</span>
        <span class:faint={!w.behind}>↓{w.behind ?? 0}</span>
      {/if}
      {#if w.merged}<span class="merged">{gt('merged')}</span>{:else if w.unpushed}<span class="warn">{gt('unpushed', { n: w.unpushed })}</span>{/if}
    </span>
    <span class="dirty" class:is={w.dirty}>{w.dirty ? gt('dirty', { n: w.changes }) : w.state === 'missing' ? '—' : gt('clean')}</span>
    <span class="num mono faint">{formatSize(w.sizeBytes)}</span>
    <span class="num faint" title={w.lastActivityAt ? new Date(w.lastActivityAt).toLocaleString() : ''}>{w.lastActivityAt ? age(w.lastActivityAt) : '—'}</span>
    <span class="acts">
      <button class="icon-btn" title={gt('openTab')} aria-label={gt('openTab')} disabled={w.state === 'missing'} onclick={() => openTab(w)}><SquareTerminal size={14} /></button>
      {#if !readonly}
        <button class="icon-btn" title={gt('review')} aria-label={gt('review')} disabled={!canReview(w)} onclick={() => review(w)}><Eye size={14} /></button>
        <button
          class="icon-btn"
          title={gt('mergeBase', { base: data?.base ?? 'main' })}
          aria-label={gt('mergeBase', { base: data?.base ?? 'main' })}
          disabled={w.kind === 'main' || w.state === 'missing' || !w.behind || busy === w.id}
          onclick={() => act(w, () => gitApi(app.requireClient()).mergeBase(w.id), gt('merged2', { base: data?.base ?? 'main', branch: w.branch ?? '' }))}><GitMerge size={14} /></button
        >
        <button class="icon-btn" title={gt('cleanup')} aria-label={gt('cleanup')} disabled={w.kind === 'main'} onclick={() => (cleaning = [w])}><BrushCleaning size={14} /></button>
      {/if}
    </span>
  </li>
{/snippet}

<section class="panel" aria-label={gt('worktrees')}>
  <div class="toolbar">
    <div class="meta">
      <span class="faint">{gt('base')}</span><span class="mono">{data?.base ?? '…'}</span>
      <span class="faint sep">{gt('store')}</span><span class="mono faint store" title={data?.store}>{data?.store ?? '…'}</span>
    </div>
    <span class="spacer"></span>
    <button class="btn" title={gt('cleanupUnusedHint')} disabled={!idle.length} onclick={() => (cleaning = idle)}><BrushCleaning size={13} />{gt('cleanupUnused', { n: idle.length })}</button>
    <button class="btn" disabled={!stale.length} onclick={() => (cleaning = stale)}><BrushCleaning size={13} />{gt('cleanupStale', { n: stale.length })}</button>
    <button class="btn" disabled={!data || busy !== null} onclick={newWorktree}><Plus size={13} />{gt('newWorktreeBtn')}</button>
    <button class="icon-btn" title={gt('refresh')} aria-label={gt('refresh')} onclick={load}><RefreshCw size={14} /></button>
  </div>
  <div class="scroll">
    {#if error}
      <p class="err pad">{error}</p>
    {:else if !data}
      <p class="faint pad">{gt('loading')}</p>
    {:else}
      <div class="cols faint">
        <span></span><span>{gt('worktrees')}</span><span>{gt('vsBase', { base: data.base })}</span><span></span><span class="num">{gt('size')}</span><span class="num">{gt('lastActivity')}</span><span></span>
      </div>
      <ul>
        {#each list as w (w.id)}{@render row(w, false)}{/each}
      </ul>
      {#if list.length <= 1}<p class="faint pad">{gt('none')}</p>{/if}
      {#if data.orphans.length}
        <h3>{gt('orphans')}</h3>
        <p class="faint hint">{gt('orphansHint')}</p>
        <ul class="orphans">
          {#each data.orphans as w (w.id)}{@render row(w, true)}{/each}
        </ul>
      {/if}
    {/if}
  </div>
</section>

{#if cleaning}
  <CleanupDialog worktrees={cleaning} onclose={() => (cleaning = null)} ondone={load} />
{/if}

<style>
  .panel {
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .toolbar .btn {
    display: inline-flex;
    gap: 5px;
    align-items: center;
  }
  .meta {
    display: flex;
    gap: 6px;
    align-items: baseline;
    min-width: 0;
    font-size: 12px;
  }
  .sep {
    margin-left: 10px;
  }
  .store {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .scroll {
    flex: 1;
    overflow: auto;
    padding: 4px 0 24px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .cols,
  .wt {
    display: grid;
    grid-template-columns: 84px minmax(0, 1fr) 150px 90px 64px 72px 128px;
    align-items: center;
    column-gap: 12px;
    padding: 0 12px 0 16px;
  }
  .cols {
    height: 28px;
    white-space: nowrap;
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }
  .wt {
    min-height: 52px;
    border-top: 1px solid var(--border);
  }
  .wt:hover {
    background: var(--surface-hover);
  }
  .state {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    font-size: 12px;
    color: var(--text-2);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--st-done);
  }
  .in_use .dot {
    background: var(--st-working);
  }
  .stale .dot {
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--st-done);
  }
  .missing .dot {
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--st-failed);
  }
  .missing .branch,
  .stale .branch {
    color: var(--text-2);
  }
  .who {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .top,
  .sub {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
  }
  .branch {
    font-size: 12.5px;
    font-weight: 500;
  }
  .small {
    font-size: 11px;
  }
  .sub {
    font-size: 11.5px;
    color: var(--text-2);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
  }
  .line {
    display: inline-flex;
    gap: 4px;
    align-items: center;
  }
  .st {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--st-done);
  }
  .st.input {
    background: var(--st-input);
  }
  .st.working {
    background: var(--st-working);
  }
  .st.held {
    background: var(--st-held);
  }
  .task {
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 260px;
  }
  .pr {
    font-size: 11px;
    height: 18px;
    padding: 0 5px;
    border-radius: 4px;
    border: 1px solid var(--border);
    color: var(--text-2);
  }
  .pr.merged {
    color: var(--st-testing);
  }
  .sync {
    display: flex;
    gap: 8px;
    font-size: 11.5px;
  }
  .merged {
    color: var(--st-testing);
    font-family: var(--font);
  }
  .warn {
    color: var(--text);
    font-family: var(--font);
  }
  .dirty {
    font-size: 12px;
    color: var(--text-3);
  }
  .dirty.is {
    color: var(--text);
  }
  .num {
    text-align: right;
    font-size: 12px;
  }
  .acts .icon-btn:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .acts {
    display: flex;
    gap: 2px;
    justify-content: flex-end;
  }
  h3 {
    margin: 22px 16px 2px;
    font-size: 12px;
    font-weight: 600;
  }
  .hint {
    margin: 0 16px 8px;
    font-size: 12px;
  }
  .orphans .wt {
    opacity: 0.85;
  }
  .pad {
    padding: 16px;
    margin: 0;
  }
  .err {
    color: var(--st-failed);
  }
  @container (max-width: 720px) {
    .wt {
      grid-template-columns: 72px minmax(0, 1fr) auto;
      padding: 6px 8px 6px 12px;
    }
    .cols,
    .sync,
    .dirty,
    .num,
    .top .small,
    .top .pr {
      display: none;
    }
    .toolbar .meta {
      flex-basis: 100%;
    }
    .branch {
      overflow: hidden;
      text-overflow: ellipsis;
    }
  }
</style>
