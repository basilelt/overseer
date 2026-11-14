<script lang="ts">
  import type { Snippet } from 'svelte';
  import { gesture } from '../links';
  import Cherry from '@lucide/svelte/icons/cherry';
  import Copy from '@lucide/svelte/icons/copy';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import FolderGit from '@lucide/svelte/icons/folder-git-2';
  import GitBranchPlus from '@lucide/svelte/icons/git-branch-plus';
  import X from '@lucide/svelte/icons/x';
  import DiffView from '../components/panel/DiffView.svelte';
  import { app } from '../state.svelte';
  import { age } from '../time';
  import { gitApi } from './api';
  import { gt } from './strings';
  import type { CommitDetail, Worktree } from './types';
  import { commitUrl } from './view';

  let { project, repo, sha, onclose, onselect, extra }: { project: string; repo?: string; sha: string; onclose: () => void; onselect?: (sha: string) => void; extra?: Snippet } = $props();

  let detail = $state<CommitDetail | null>(null);
  let error = $state<string | null>(null);
  let mode = $state<'none' | 'branch' | 'cherry'>('none');
  let branchName = $state('');
  let targets = $state<Worktree[]>([]);
  let target = $state('');
  let busy = $state(false);

  const short = $derived(sha.slice(0, 7));
  const url = $derived(commitUrl(repo, sha));
  const err = (e: unknown) => String((e as Error)?.message ?? e);
  const api = () => gitApi(app.requireClient());

  $effect(() => {
    const s = sha;
    detail = null;
    error = null;
    mode = 'none';
    api()
      .commit(project, s)
      .then((d) => {
        if (s === sha) detail = d;
      })
      .catch((e) => (error = err(e)));
  });

  async function run(fn: () => Promise<string>) {
    busy = true;
    try {
      app.notify(await fn());
      mode = 'none';
    } catch (e) {
      app.notify(err(e), 'error');
    } finally {
      busy = false;
    }
  }

  const newWorktree = () =>
    run(async () => {
      const w = await api().createWorktree({ project, base: sha });
      return gt('worktreeCreated', { branch: w.branch ?? w.path });
    });

  const createBranch = () =>
    run(async () => {
      const b = await api().createBranch(project, branchName.trim(), sha);
      return gt('branchCreated', { name: b.name, sha: short });
    });

  async function openCherry() {
    mode = 'cherry';
    target = '';
    try {
      const l = await api().worktrees(project);
      targets = l.worktrees.filter((w) => w.state !== 'missing' && w.branch);
    } catch (e) {
      app.notify(err(e), 'error');
    }
  }

  const cherry = () =>
    run(async () => {
      const w = targets.find((x) => x.id === target)!;
      const r = await api().cherryPick(w.id, sha);
      return gt('cherryDone', { sha: short, branch: w.branch ?? w.path, commit: r.commit.slice(0, 7) });
    });

  async function copy() {
    try {
      await navigator.clipboard.writeText(sha);
      app.notify(gt('copied', { sha: short }));
    } catch (e) {
      app.notify(err(e), 'error');
    }
  }

  const statusClass = (s: string) => ({ A: 'add', D: 'del', R: 'ren' })[s[0]] ?? 'mod';
</script>

<aside class="detail" aria-label={short}>
  <header>
    <span class="mono sha">{short}</span>
    <span class="spacer"></span>
    <button class="icon-btn" title={gt('copySha')} aria-label={gt('copySha')} onclick={copy}><Copy size={14} /></button>
    {#if url}<button class="icon-btn" title={gt('openGithub')} aria-label={gt('openGithub')} onclick={(e) => app.openUrl(url!, undefined, gesture(e))}><ExternalLink size={14} /></button>{/if}
    <button class="icon-btn" title={gt('close')} aria-label={gt('close')} onclick={onclose}><X size={15} /></button>
  </header>
  <div class="scroll">
    {#if error}
      <p class="err">{error}</p>
    {:else if !detail}
      <p class="faint">{gt('loading')}</p>
    {:else}
      <h3>{detail.subject}</h3>
      <p class="meta">
        <span>{detail.author}</span>
        <span class="faint">·</span>
        <span class="faint" title={new Date(detail.at).toLocaleString()}>{age(detail.at)}</span>
      </p>
      {#if detail.refs.length}
        <div class="chips">{#each detail.refs as r (r)}<span class="chip">{r}</span>{/each}</div>
      {/if}
      {#if detail.body.trim()}<pre class="body">{detail.body.trim()}</pre>{/if}
      <p class="parents faint">
        {gt('parents')}
        {#each detail.parents as p (p)}<button class="mono link" onclick={() => onselect?.(p)}>{p.slice(0, 7)}</button>{/each}
      </p>

      <div class="actions">
        <button class="btn" disabled={busy} onclick={newWorktree}><FolderGit size={13} />{gt('newWorktree')}</button>
        <button class="btn" class:on={mode === 'branch'} onclick={() => (mode = mode === 'branch' ? 'none' : 'branch')}><GitBranchPlus size={13} />{gt('createBranch')}</button>
        <button class="btn" class:on={mode === 'cherry'} onclick={() => (mode === 'cherry' ? (mode = 'none') : openCherry())}><Cherry size={13} />{gt('cherryPick')}</button>
        {@render extra?.()}
      </div>

      {#if mode === 'branch'}
        <form class="inline" onsubmit={(e) => { e.preventDefault(); if (branchName.trim()) createBranch(); }}>
          <input class="mono" placeholder={gt('branchName')} bind:value={branchName} spellcheck="false" />
          <button class="btn primary" disabled={busy || !branchName.trim()}>{gt('createBranch')}</button>
        </form>
      {:else if mode === 'cherry'}
        <div class="inline col">
          <label class="faint" for="cherry-target">{gt('cherryInto')}</label>
          <select id="cherry-target" bind:value={target}>
            <option value="" disabled>{gt('pickTarget')}</option>
            {#each targets as w (w.id)}<option value={w.id}>{w.branch}{w.dirty ? ` · ${gt('dirty', { n: w.changes })}` : ''}</option>{/each}
          </select>
          {#if target}
            <button class="btn primary" disabled={busy} onclick={cherry}>{gt('confirmCherry', { sha: short, branch: targets.find((w) => w.id === target)?.branch ?? '' })}</button>
          {/if}
        </div>
      {/if}

      <h4>{detail.files.length === 1 ? gt('file1') : detail.files.length ? gt('files', { n: detail.files.length }) : gt('noFiles')}</h4>
      {#if detail.files.length}
        <ul class="files">
          {#each detail.files as f (f.path)}
            <li>
              <span class="status {statusClass(f.status)} mono">{f.status[0]}</span>
              <span class="path mono" title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path}>{f.path}</span>
              {#if !f.binary}<span class="add mono">+{f.added}</span><span class="del mono">−{f.removed}</span>{/if}
            </li>
          {/each}
        </ul>
        <DiffView diff={detail.diff} />
        {#if detail.truncated}<p class="faint">{gt('truncated')}</p>{/if}
      {/if}
    {/if}
  </div>
</aside>

<style>
  .detail {
    width: 460px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-left: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  header {
    display: flex;
    align-items: center;
    height: 37px;
    padding: 0 8px 0 14px;
    border-bottom: 1px solid var(--border);
  }
  .sha {
    font-size: 12px;
    color: var(--text-2);
  }
  .spacer {
    flex: 1;
  }
  .scroll {
    flex: 1;
    overflow: auto;
    padding: 14px;
    user-select: text;
    -webkit-user-select: text;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 14px;
    font-weight: 600;
    line-height: 1.35;
  }
  h4 {
    margin: 18px 0 8px;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }
  .meta {
    display: flex;
    gap: 6px;
    margin: 0 0 10px;
    color: var(--text-2);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 10px;
  }
  .chip {
    height: 18px;
    padding: 0 6px;
    border-radius: 4px;
    border: 1px solid var(--border-strong);
    font-size: 11px;
    color: var(--text-2);
  }
  .body {
    margin: 0 0 10px;
    white-space: pre-wrap;
    font-family: var(--font);
    color: var(--text-2);
  }
  .parents {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 0 0 12px;
    font-size: 12px;
  }
  .link {
    color: var(--text-2);
    font-size: 11.5px;
  }
  .link:hover {
    color: var(--accent);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .actions .btn {
    display: inline-flex;
    gap: 5px;
    align-items: center;
  }
  .btn.on {
    background: var(--surface-active);
  }
  .inline {
    display: flex;
    gap: 6px;
    margin-top: 10px;
  }
  .inline.col {
    flex-direction: column;
    align-items: stretch;
  }
  .inline input,
  .inline select {
    flex: 1;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    font-size: 12px;
  }
  .files {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
  }
  .files li {
    display: flex;
    gap: 8px;
    align-items: center;
    height: 22px;
    font-size: 12px;
  }
  .status {
    width: 14px;
    text-align: center;
    font-size: 11px;
    color: var(--text-3);
  }
  .status.add {
    color: var(--diff-add);
  }
  .status.del {
    color: var(--diff-del);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .add {
    color: var(--diff-add);
    font-size: 11px;
  }
  .del {
    color: var(--diff-del);
    font-size: 11px;
  }
  .err {
    color: var(--st-failed);
  }
</style>
