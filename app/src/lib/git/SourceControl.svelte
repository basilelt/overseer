<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Check from '@lucide/svelte/icons/check';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import Minus from '@lucide/svelte/icons/minus';
  import Plus from '@lucide/svelte/icons/plus';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import { app } from '../state.svelte';
  import { openReview, openReviewBeside } from '../review/open';
  import { isPhone } from '../mobile/ui.svelte';
  import { fileCount, touches, type Scm } from './scm.svelte';
  import { gt } from './strings';
  import type { ScmFile } from './types';
  import { isSubmitChord } from '../chord';

  let { scm, filesOnly = false, onopen }: { scm: Scm; filesOnly?: boolean; onopen?: (worktree: string) => void } = $props();

  let message = $state('');
  let newBranch = $state<string | null>(null);
  let confirming = $state<{ files: string[] | 'all'; label: string } | null>(null);

  const s = $derived(scm.status);
  const busy = $derived(!!scm.busy);
  const total = $derived(fileCount(s));
  const commitLabel = $derived(
    !s ? gt('sc.commit') : s.staged.length ? gt('sc.commitN', { n: s.staged.length }) : s.changes.length ? gt('sc.commitAll', { n: s.changes.length }) : gt('sc.commit'),
  );
  const canCommit = $derived(!!s && !busy && !!message.trim() && (s.staged.length > 0 || s.changes.length > 0) && !s.conflicts.length);

  $effect(() => {
    const phase = app.conn.phase;
    if (!filesOnly) untrack(() => scm.connection(phase));
  });

  onMount(() => {
    if (filesOnly) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const off = app.onChange((_p, _n, e) => {
      if (!touches(scm.status, e)) return;
      clearTimeout(timer);
      timer = setTimeout(() => scm.load(), 300);
    });
    return () => {
      off();
      clearTimeout(timer);
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

  function commitMenu(e: MouseEvent) {
    app.openMenu(e, gt('sc.commitOptions'), [
      { label: gt('sc.commitPush'), disabled: !canCommit, run: () => commit({ push: true }) },
      { label: gt('sc.commitAllOpt'), disabled: !canCommit, run: () => commit({ all: true }) },
      { label: gt('sc.amend'), disabled: busy || !s?.head, run: () => commit({ amend: true }) },
    ]);
  }

  function branchMenu(e: MouseEvent) {
    if (!s) return;
    app.openMenu(e, gt('sc.switchBranch'), [
      ...s.branches.map((b) => ({ label: b, checked: b === s.branch, disabled: b === s.branch, run: () => scm.switchTo(b) })),
      { label: '', separator: true },
      { label: gt('sc.newBranch'), run: () => (newBranch = '') },
    ]);
  }

  function mergeMenu(e: MouseEvent) {
    if (!s) return;
    app.openMenu(
      e,
      gt('sc.mergeInto', { branch: s.branch ?? 'HEAD' }),
      s.branches.filter((b) => b !== s.branch).map((b) => ({ label: b, run: () => scm.merge(b) })),
    );
  }

  async function createBranch() {
    const name = newBranch?.trim();
    if (!name) return;
    if (await scm.switchTo(name, true)) newBranch = null;
  }

  function open() {
    if (!s) return;
    if (isPhone()) openReview({ worktree: s.worktree });
    else openReviewBeside({ worktree: s.worktree });
  }

  function onKey(e: KeyboardEvent) {
    if (isSubmitChord(e) && canCommit) {
      e.preventDefault();
      commit();
    }
  }

  async function discard() {
    if (!confirming) return;
    const files = confirming.files;
    confirming = null;
    await scm.discard(files);
  }

  const subOf = (f: ScmFile) => s?.submodules?.find((m) => m.path === f.path);
  const name = (f: ScmFile) => f.path.split('/').pop() ?? f.path;
  const dir = (f: ScmFile) => f.path.split('/').slice(0, -1).join('/');
  const stateClass = (st: string) => (st === '?' || st === 'A' ? 'add' : st === 'D' ? 'del' : st === 'U' ? 'conf' : 'mod');
</script>

<div class="scm" aria-busy={busy || scm.loading}>
  {#if scm.error && !s}
    <p class="err pad">{scm.error}</p>
    <button class="btn ghost retry" onclick={() => scm.load()}><RefreshCw size={13} />{gt('refresh')}</button>
  {:else if !s}
    <p class="faint pad">{gt('loading')}</p>
  {:else}
    {#if !filesOnly}
    <div class="block">
      {#if newBranch !== null}
        <form class="newb" onsubmit={(e) => { e.preventDefault(); createBranch(); }}>
          <input class="mono" placeholder={gt('branchName')} aria-label={gt('branchName')} bind:value={newBranch} spellcheck="false" />
          <button class="btn primary" type="submit" disabled={busy || !newBranch.trim()}>{gt('createBranch')}</button>
          <button class="btn ghost" type="button" onclick={() => (newBranch = null)}>{gt('sc.cancel')}</button>
        </form>
      {:else}
        <button class="branch mono" title={gt('sc.switchBranch')} aria-haspopup="menu" onclick={branchMenu} disabled={busy}>
          <GitBranch size={13} /><span class="bname">{s.branch ?? gt('sc.detached')}</span><ChevronDown size={12} />
        </button>
      {/if}
      <div class="sync">
        <button class="btn" onclick={() => scm.fetch()} disabled={busy}><RefreshCw size={12} />{gt('sc.fetch')}</button>
        <button class="btn" onclick={() => scm.pull()} disabled={busy || !s.upstream}><ArrowDown size={12} />{gt('sc.pull')}{#if s.behind}<span class="n">{s.behind}</span>{/if}</button>
        <button class="btn" onclick={() => scm.push()} disabled={busy || !s.branch}><ArrowUp size={12} />{s.upstream ? gt('sc.push') : gt('sc.publish')}{#if s.upstream && s.ahead}<span class="n">{s.ahead}</span>{/if}</button>
      </div>
      {#if s.operation}<p class="op">{gt(`sc.op.${s.operation}`)}</p>{/if}
    </div>

    <div class="block">
      <label class="msg">
        <span class="lbl">{gt('sc.message')}</span>
        <textarea rows="3" bind:value={message} onkeydown={onKey} placeholder={gt('sc.messageHint')} disabled={busy}></textarea>
      </label>
      <div class="split">
        <button class="btn primary main" onclick={() => commit()} disabled={!canCommit}><Check size={13} />{commitLabel}</button>
        <button class="btn primary more" aria-label={gt('sc.commitOptions')} title={gt('sc.commitOptions')} aria-haspopup="menu" onclick={commitMenu} disabled={busy}><ChevronDown size={12} /></button>
      </div>
    </div>
    {/if}

    {#if confirming}
      <div class="confirm" role="alertdialog" aria-label={gt('sc.discardTitle')}>
        <span>{confirming.label}</span>
        <span class="row-actions">
          <button class="btn ghost" onclick={() => (confirming = null)}>{gt('sc.cancel')}</button>
          <button class="btn danger" onclick={discard}>{gt('sc.discard')}</button>
        </span>
      </div>
    {/if}

    <div class="files">
      {#if !total}<p class="faint pad">{gt('sc.clean')}</p>{/if}
      {#if s.conflicts.length}
        <div class="sec"><span>{gt('sc.conflicts')}</span><span class="cnt">{s.conflicts.length}</span></div>
        {#each s.conflicts as f (f.path)}
          <div class="file"><span class="st conf">U</span><button class="fname" onclick={open}>{name(f)}<span class="dir">{dir(f)}</span></button>
            <span class="acts"><button class="icon-btn sm" title={gt('sc.stage')} aria-label={gt('sc.stage')} onclick={() => scm.stage([f.path])} disabled={busy}><Plus size={13} /></button></span></div>
        {/each}
      {/if}
      {#if s.staged.length}
        <div class="sec"><span>{gt('sc.staged')}</span><span class="cnt">{s.staged.length}</span>
          <button class="icon-btn sm" title={gt('sc.unstageAll')} aria-label={gt('sc.unstageAll')} onclick={() => scm.unstage('all')} disabled={busy}><Minus size={13} /></button></div>
        {#each s.staged as f (f.path)}
          <div class="file"><span class="st {stateClass(f.status)}">{f.status}</span><button class="fname" onclick={open}>{name(f)}<span class="dir">{dir(f)}</span></button>
            <span class="acts"><button class="icon-btn sm" title={gt('sc.unstage')} aria-label={gt('sc.unstage')} onclick={() => scm.unstage([f.path])} disabled={busy}><Minus size={13} /></button></span></div>
        {/each}
      {/if}
      {#if s.changes.length}
        <div class="sec"><span>{gt('sc.changes')}</span><span class="cnt">{s.changes.length}</span>
          <button class="icon-btn sm" title={gt('sc.discardAll')} aria-label={gt('sc.discardAll')} onclick={() => (confirming = { files: 'all', label: gt('sc.discardAllQ', { n: total }) })} disabled={busy}><Undo2 size={13} /></button>
          <button class="icon-btn sm" title={gt('sc.stageAll')} aria-label={gt('sc.stageAll')} onclick={() => scm.stage('all')} disabled={busy}><Plus size={13} /></button></div>
        {#each s.changes as f (f.path)}
          {#if f.submodule}
            {@const sub = subOf(f)}
            <div class="file"><span class="st mod">M</span><span class="fname">{name(f)}<span class="dir">{f.submodule.commit ? gt('sc.subCommit') : gt('sc.subInside')}</span></span>
              <span class="acts show">
                {#if f.submodule.commit}<button class="btn ghost sm" onclick={() => scm.stage([f.path])} disabled={busy}>{gt('sc.stagePointer')}</button>{/if}
                {#if sub && onopen && (f.submodule.modified || f.submodule.untracked)}<button class="btn ghost sm" onclick={() => onopen(sub.worktree)}>{gt('sc.openSub', { name: name(f) })}</button>{/if}
              </span></div>
          {:else}
          <div class="file"><span class="st {stateClass(f.status)}">{f.status === '?' ? 'U' : f.status}</span><button class="fname" onclick={open}>{name(f)}<span class="dir">{dir(f)}</span></button>
            <span class="acts">
              <button class="icon-btn sm" title={gt('sc.discard')} aria-label={gt('sc.discardFile', { file: f.path })} onclick={() => (confirming = { files: [f.path], label: gt('sc.discardQ', { file: f.path }) })} disabled={busy}><Undo2 size={13} /></button>
              <button class="icon-btn sm" title={gt('sc.stage')} aria-label={gt('sc.stageFile', { file: f.path })} onclick={() => scm.stage([f.path])} disabled={busy}><Plus size={13} /></button>
            </span></div>
          {/if}
        {/each}
      {/if}
      {#if s.outgoing.length && !filesOnly}
        <div class="sec"><span>{s.upstream ? gt('sc.outgoing') : gt('sc.outgoingBase', { base: s.base })}</span><span class="cnt">{s.outgoing.length}</span></div>
        {#each s.outgoing as c (c.sha)}<div class="commit"><span class="mono sha">{c.sha.slice(0, 7)}</span><span class="subj">{c.subject}</span></div>{/each}
      {/if}
      {#if s.incoming.length && !filesOnly}
        <div class="sec"><span title={s.upstream ?? ''}>{gt('sc.incoming', { upstream: s.upstream ?? '' })}</span><span class="cnt">{s.incoming.length}</span></div>
        {#each s.incoming as c (c.sha)}<div class="commit"><span class="mono sha">{c.sha.slice(0, 7)}</span><span class="subj">{c.subject}</span></div>{/each}
      {/if}
    </div>

    {#if !filesOnly}
    <div class="foot">
      {#if s.branch !== s.base}<button class="btn ghost" onclick={() => scm.merge(s.base)} disabled={busy}>{gt('mergeBase', { base: s.base })}</button>{/if}
      <button class="btn ghost" aria-haspopup="menu" onclick={mergeMenu} disabled={busy || s.branches.length < 2}>{gt('sc.merge')}</button>
      <button class="btn ghost" onclick={() => scm.stash()} disabled={busy || !total}>{gt('sc.stash')}</button>
      {#if s.stashes}<button class="btn ghost" onclick={() => scm.pop()} disabled={busy}>{gt('sc.pop', { n: s.stashes })}</button>{/if}
    </div>
    {/if}
  {/if}
</div>

<style>
  .scm {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    font-size: 13px;
  }
  .pad {
    padding: 12px 14px;
    margin: 0;
  }
  .err {
    color: var(--st-failed);
  }
  .retry {
    margin: 0 14px;
    align-self: flex-start;
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .branch {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
    text-align: left;
  }
  .bname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .newb {
    display: flex;
    gap: 6px;
  }
  .newb input {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
  }
  .sync {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }
  .btn {
    justify-content: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 400;
  }
  .btn.primary {
    font-weight: 500;
  }
  .foot .btn {
    border-color: var(--border);
  }
  .n {
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  .op {
    margin: 0;
    font-size: 12px;
    color: var(--st-paused);
  }
  .msg {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lbl {
    font-size: 12px;
    color: var(--text-2);
  }
  textarea {
    resize: vertical;
    min-height: 54px;
    padding: 7px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font: 13px/1.4 var(--font);
  }
  textarea:focus {
    outline: none;
    border-color: var(--accent);
  }
  .split {
    display: flex;
  }
  .split .main {
    flex: 1;
    border-radius: var(--radius) 0 0 var(--radius);
  }
  .split .more {
    width: 30px;
    padding: 0;
    border-radius: 0 var(--radius) var(--radius) 0;
    border-left-color: color-mix(in srgb, var(--accent-fg) 30%, transparent);
  }
  .confirm {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 8px 12px 0;
    padding: 10px;
    border: 1px solid var(--st-failed);
    border-radius: var(--radius);
    background: var(--diff-del-bg);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .row-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .files {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-bottom: 8px;
  }
  .sec {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px 4px 14px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .sec span:first-child {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cnt {
    font-weight: 400;
    color: var(--text-3);
  }
  .file,
  .commit {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
    padding: 0 8px 0 14px;
  }
  .file:hover {
    background: var(--surface-hover);
  }
  .st {
    width: 12px;
    flex: none;
    font: 11px var(--mono);
    text-align: center;
  }
  .st.mod {
    color: var(--st-paused);
  }
  .st.add {
    color: var(--st-input);
  }
  .st.del {
    color: var(--st-failed);
  }
  .st.conf {
    color: var(--st-testing);
  }
  .fname {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 6px;
    align-items: baseline;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font-size: 13px;
    text-align: left;
    overflow: hidden;
    white-space: nowrap;
  }
  .dir {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-3);
    font-size: 12px;
  }
  .acts {
    display: flex;
    gap: 2px;
    opacity: 0;
  }
  .file:hover .acts,
  .file:focus-within .acts,
  .acts.show {
    opacity: 1;
  }
  .btn.sm {
    height: 22px;
    padding: 0 7px;
    font-size: 11.5px;
  }
  @media (hover: none) {
    .acts {
      opacity: 1;
    }
  }
  .icon-btn.sm {
    width: 22px;
    height: 22px;
  }
  .sha {
    font-size: 11px;
    color: var(--text-3);
    flex: none;
  }
  .subj {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 10px 12px;
    border-top: 1px solid var(--border);
  }
</style>
