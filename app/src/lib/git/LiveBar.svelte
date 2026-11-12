<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Check from '@lucide/svelte/icons/check';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import { app } from '../state.svelte';
  import { beside } from '../review/beside.svelte';
  import { reviewApi } from '../review/api';
  import { folder } from '../review/scope';
  import { worktreeChoices } from './worktrees';
  import type { Related } from '../review/types';
  import type { MenuItem } from '../commands/menu';
  import { gitApi } from './api';
  import { live, liveTarget, sessionTarget } from './live.svelte';
  import { Scm, fileCount, primaryAction, touches } from './scm.svelte';
  import SourceControl from './SourceControl.svelte';
  import { gt } from './strings';
  import type { ScmSubmodule } from './types';
  import { isSubmitChord } from '../chord';

  const scm = new Scm(
    () => (app.client ? gitApi(app.client) : null),
    (text) => app.notify(text, 'error'),
  );

  let message = $state('');
  let newBranch = $state<string | null>(null);
  let confirmAll = $state(false);
  let filesOpen = $state(false);
  let related = $state.raw<Related | null>(null);
  let subs = $state.raw<Record<string, ScmSubmodule[]>>({});
  let input: HTMLInputElement | undefined = $state();

  const target = $derived(liveTarget());
  const s = $derived(scm.status);
  const busy = $derived(!!scm.busy);
  const total = $derived(fileCount(s));
  const primary = $derived(primaryAction(s, message));
  const session = $derived(sessionTarget());
  const running = $derived(!!session?.lineId && app.items.some((i) => i.lineId === session.lineId && i.state === 'running'));
  const name = $derived(s ? folder(s.worktree) : '');

  $effect(() => {
    scm.setTarget(target);
  });

  let lastKey = untrack(() => beside.key);
  $effect(() => {
    const k = beside.key;
    if (k === lastKey) return;
    lastKey = k;
    untrack(() => {
      live.worktree = null;
      filesOpen = false;
      confirmAll = false;
    });
  });

  $effect(() => {
    const t = session;
    if (!t || !app.client) {
      related = null;
      return;
    }
    const api = reviewApi(app.client);
    untrack(() => {
      api
        .related(t)
        .then((r) => (related = r))
        .catch(() => (related = null));
    });
  });

  $effect(() => {
    const st = s;
    if (!st?.submodules?.length || subs[st.worktree]) return;
    untrack(() => (subs = { ...subs, [st.worktree]: st.submodules }));
  });

  $effect(() => {
    const phase = app.conn.phase;
    untrack(() => scm.connection(phase));
  });

  onMount(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const off = app.onChange((_p, _n, e) => {
      if (!touches(scm.status, e)) return;
      clearTimeout(timer);
      timer = setTimeout(() => scm.load(), 300);
    });
    const away = (e: PointerEvent) => {
      if (filesOpen && !(e.target as HTMLElement).closest('.files-pop,.files-btn')) filesOpen = false;
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        filesOpen = false;
        confirmAll = false;
        newBranch = null;
      }
    };
    window.addEventListener('pointerdown', away);
    window.addEventListener('keydown', esc);
    return () => {
      off();
      clearTimeout(timer);
      window.removeEventListener('pointerdown', away);
      window.removeEventListener('keydown', esc);
    };
  });

  async function commit(o: { push?: boolean; amend?: boolean; all?: boolean } = {}) {
    if (!s) return;
    const all = o.all || !s.staged.length;
    const r = await scm.commit({ message: message.trim(), amend: o.amend, push: o.push, all: all || undefined });
    if (r) {
      message = '';
      app.notify(gt('sc.committed', { sha: (r.commit ?? '').slice(0, 7) }));
    }
  }

  function runPrimary() {
    const k = primary.kind;
    if (k === 'commit' || k === 'merge') commit();
    else if (k === 'publish' || k === 'push') scm.push();
    else if (k === 'pull') scm.pull();
  }

  const primaryLabel = $derived.by(() => {
    const p = primary;
    if (p.kind === 'merge') return gt('live.commitMerge');
    if (p.kind === 'commit') return s?.staged.length ? gt('live.commitN', { n: p.n }) : gt('sc.commitAll', { n: p.n });
    if (p.kind === 'publish') return gt('live.publish');
    if (p.kind === 'pull') return gt('live.pull', { n: p.n });
    if (p.kind === 'push') return gt('live.push', { n: p.n });
    return gt('live.upToDate');
  });

  function onKey(e: KeyboardEvent) {
    if ((isSubmitChord(e) || (e.key === 'Enter' && !e.shiftKey)) && (primary.kind === 'commit' || primary.kind === 'merge') && primary.enabled) {
      e.preventDefault();
      commit();
    }
  }

  function worktreeMenu(e: MouseEvent) {
    const items: MenuItem[] = worktreeChoices(related, subs, s?.worktree).map((w) =>
      w.submodule
        ? { label: `   ↳ ${gt('live.submodule', { name: w.name })}`, checked: w.current, run: () => (live.worktree = w.path) }
        : { label: `${w.name} · ${w.branch ?? 'HEAD'}`, hint: w.files ? gt('live.changed', { n: w.files }) : gt('live.clean'), checked: w.current, run: () => (live.worktree = w.path) },
    );
    if (s) {
      items.push({ label: '', separator: true });
      items.push({ label: gt('sc.switchBranch'), disabled: true });
      for (const b of s.branches) items.push({ label: `   ${b}`, checked: b === s.branch, disabled: b === s.branch || busy, run: () => scm.switchTo(b) });
      items.push({ label: gt('sc.newBranch'), disabled: busy, run: () => (newBranch = '') });
    }
    app.openMenu(e, gt('worktree'), items);
  }

  function moreMenu(e: MouseEvent) {
    if (!s) return;
    const canCommit = !!message.trim() && total > 0 && !s.conflicts.length;
    app.openMenu(e, gt('live.more'), [
      { label: gt('sc.fetch'), disabled: busy, run: () => scm.fetch() },
      { label: s.behind ? gt('live.pull', { n: s.behind }) : gt('sc.pull'), disabled: busy || !s.upstream, run: () => scm.pull() },
      { label: s.upstream ? (s.ahead ? gt('live.push', { n: s.ahead }) : gt('sc.push')) : gt('live.publish'), disabled: busy || !s.branch, run: () => scm.push() },
      { label: '', separator: true },
      { label: gt('sc.commitPush'), disabled: busy || !canCommit, run: () => commit({ push: true }) },
      { label: gt('sc.commitAllOpt'), disabled: busy || !canCommit, run: () => commit({ all: true }) },
      { label: gt('sc.amend'), disabled: busy || !s.head, run: () => commit({ amend: true }) },
      { label: gt('sc.stageAll'), disabled: busy || !s.changes.length, run: () => scm.stage('all') },
      { label: gt('sc.unstageAll'), disabled: busy || !s.staged.length, run: () => scm.unstage('all') },
      { label: gt('live.discardAll'), danger: true, disabled: busy || !total, run: () => (confirmAll = true) },
      { label: '', separator: true },
      { label: gt('mergeBase', { base: s.base }), disabled: busy || s.branch === s.base, run: () => scm.merge(s.base) },
      { label: gt('sc.merge'), disabled: busy || s.branches.length < 2, run: () => mergeMenu(e) },
      { label: gt('sc.stash'), disabled: busy || !total, run: () => scm.stash() },
      { label: gt('sc.pop', { n: s.stashes }), disabled: busy || !s.stashes, run: () => scm.pop() },
      { label: '', separator: true },
      { label: gt('live.openGraph'), run: openGraph },
    ]);
  }

  function mergeMenu(e: MouseEvent) {
    if (!s) return;
    app.openMenu(e, gt('sc.mergeInto', { branch: s.branch ?? 'HEAD' }), s.branches.filter((b) => b !== s.branch).map((b) => ({ label: b, run: () => scm.merge(b) })));
  }

  function openGraph() {
    if (!s) return;
    app.gitWorktree = { project: s.project, path: s.worktree };
    app.openProjectTab('graph', s.project);
  }

  async function createBranch() {
    const n = newBranch?.trim();
    if (!n) return;
    if (await scm.switchTo(n, true)) newBranch = null;
  }

  async function discardAll() {
    confirmAll = false;
    await scm.discard('all');
  }
</script>

<footer class="live" aria-label={gt('live.bar')} data-tour="live-bar">
  {#if scm.error && !s}
    <span class="err">{scm.error}</span>
  {:else if s}
    <button class="wt" title={s.worktree} aria-haspopup="menu" onclick={worktreeMenu} disabled={busy}>
      <GitBranch size={13} />
      <span class="nm">{name}</span>
      {#if !s.branch || s.branch.replaceAll('/', '-') !== name}<span class="br mono">{s.branch ?? gt('sc.detached')}</span>{/if}
      <ChevronDown size={12} />
    </button>
    <button class="files-btn" class:on={filesOpen} aria-expanded={filesOpen} onclick={() => (filesOpen = !filesOpen)} disabled={!total}>
      {#if total}
        <span>{gt('live.changed', { n: total })}</span>{#if s.staged.length}<span class="muted">· {gt('live.staged', { n: s.staged.length })}</span>{/if}
        <ChevronUp size={12} />
      {:else}
        <span class="muted">{gt('live.clean')}</span>
      {/if}
    </button>
    {#if s.operation}
      <span class="warn">{gt(`sc.op.${s.operation}`)}</span>
    {:else if running && total}
      <span class="warn">● {gt('live.agentRunning')}</span>
    {/if}
    <span class="spacer"></span>
    {#if confirmAll}
      <div class="confirm" role="alertdialog" aria-label={gt('sc.discardTitle')}>
        <span>{gt('sc.discardAllQ', { n: total })}</span>
        <button class="btn ghost" onclick={() => (confirmAll = false)}>{gt('sc.cancel')}</button>
        <button class="btn danger" onclick={discardAll}>{gt('sc.discard')}</button>
      </div>
    {:else if newBranch !== null}
      <form class="row" onsubmit={(e) => { e.preventDefault(); createBranch(); }}>
        <input class="msg mono" placeholder={gt('branchName')} aria-label={gt('branchName')} bind:value={newBranch} spellcheck="false" />
        <button class="btn ghost" type="button" onclick={() => (newBranch = null)}>{gt('sc.cancel')}</button>
        <button class="btn primary" type="submit" disabled={busy || !newBranch.trim()}>{gt('createBranch')}</button>
      </form>
    {:else}
      <input class="msg" bind:this={input} bind:value={message} onkeydown={onKey} placeholder={gt('live.messageHint')} aria-label={gt('sc.message')} disabled={busy || !(total || s.operation)} spellcheck="true" />
      <button class="btn primary main" onclick={runPrimary} disabled={busy || !primary.enabled}>
        {#if primary.kind === 'push' || primary.kind === 'publish'}<ArrowUp size={13} />{:else if primary.kind === 'pull'}<ArrowDown size={13} />{:else}<Check size={13} />{/if}
        {primaryLabel}
      </button>
    {/if}
    <button class="btn more" aria-label={gt('live.more')} title={gt('live.more')} aria-haspopup="menu" onclick={moreMenu} disabled={busy}><Ellipsis size={15} /></button>
    {#if filesOpen}
      <div class="files-pop" role="dialog" aria-label={gt('sc.changes')}>
        <SourceControl {scm} filesOnly onopen={(w) => { live.worktree = w; filesOpen = false; }} />
      </div>
    {/if}
  {:else}
    <span class="muted">{gt('loading')}</span>
  {/if}
</footer>

<style>
  .live {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 12px 0 10px;
    border-top: 1px solid var(--border);
    background: var(--bg-subtle);
    font-size: 13px;
  }
  .wt,
  .files-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
    min-width: 0;
  }
  .wt:hover:not(:disabled),
  .files-btn:hover:not(:disabled),
  .files-btn.on {
    background: var(--surface-hover);
    border-color: var(--border-strong);
  }
  .wt .nm {
    font-weight: 600;
  }
  .wt .br {
    font-size: 12px;
    color: var(--text-2);
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .files-btn:disabled {
    cursor: default;
  }
  .muted {
    color: var(--text-3);
  }
  .warn {
    color: var(--st-paused);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .err {
    color: var(--st-failed);
  }
  .spacer {
    flex: 1;
  }
  .msg {
    width: min(380px, 32vw);
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--text);
    font: 13px var(--font);
  }
  .msg:focus {
    outline: none;
    border-color: var(--accent);
  }
  .msg:disabled {
    opacity: 0.6;
  }
  .row,
  .confirm {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .confirm span {
    color: var(--st-failed);
    font-size: 12.5px;
    max-width: 420px;
  }
  .btn {
    height: 32px;
    font-size: 12.5px;
  }
  .btn.main {
    min-width: 128px;
    justify-content: center;
  }
  .btn.more {
    width: 34px;
    padding: 0;
    justify-content: center;
  }
  .btn.danger {
    color: #fff;
    background: var(--st-failed);
    border-color: var(--st-failed);
  }
  .files-pop {
    position: absolute;
    left: 10px;
    bottom: calc(100% + 6px);
    width: 400px;
    max-width: calc(100vw - 24px);
    max-height: min(60vh, 520px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow);
    overflow: auto;
    z-index: 20;
    padding-bottom: 6px;
  }
</style>
