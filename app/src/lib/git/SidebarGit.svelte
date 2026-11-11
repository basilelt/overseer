<script lang="ts">
  import { untrack } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import FolderGit from '@lucide/svelte/icons/folder-git-2';
  import { app } from '../state.svelte';
  import { itemTarget } from '../review/open';
  import { reviewApi } from '../review/api';
  import type { Related } from '../review/types';
  import { gitApi } from './api';
  import { Scm, targetKey } from './scm.svelte';
  import SourceControl from './SourceControl.svelte';
  import { gt } from './strings';
  import type { ScmSubmodule, ScmTarget } from './types';
  import { worktreeChoices } from './worktrees';

  const scm = new Scm(
    () => (app.client ? gitApi(app.client) : null),
    (text) => app.notify(text, 'error'),
  );

  let picked = $state<string | null>(null);
  let related = $state.raw<Related | null>(null);
  let subs = $state.raw<Record<string, ScmSubmodule[]>>({});

  const session = $derived(itemTarget(app.selectedKey));
  const target = $derived<ScmTarget | null>(picked ? { worktree: picked } : session);
  const s = $derived(scm.status);
  const choices = $derived(worktreeChoices(related, subs, s?.worktree));
  const current = $derived(choices.find((c) => c.current));

  let lastSession = untrack(() => targetKey(session));
  $effect(() => {
    const k = targetKey(session);
    if (k === lastSession) return;
    lastSession = k;
    untrack(() => (picked = null));
  });

  $effect(() => {
    scm.setTarget(target);
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

  function menu(e: MouseEvent) {
    app.openMenu(
      e,
      gt('worktree'),
      choices.map((w) => ({
        label: w.submodule ? `   ↳ ${gt('live.submodule', { name: w.name })}` : `${w.name} · ${w.branch ?? 'HEAD'}`,
        hint: w.submodule ? undefined : w.files ? gt('live.changed', { n: w.files }) : gt('live.clean'),
        checked: w.current,
        run: () => (picked = w.path),
      })),
    );
  }
</script>

<section class="side-git" aria-label={gt('scm')} data-tour="sidebar-git">
  {#if !target}
    <p class="empty">{gt('side.empty')}</p>
  {:else}
    <button class="wt" title={s?.worktree ?? ''} aria-haspopup="menu" onclick={menu} disabled={choices.length < 2}>
      <FolderGit size={14} />
      <span class="nm">{current ? (current.submodule ? gt('live.submodule', { name: current.name }) : current.name) : '…'}</span>
      {#if choices.length > 1}<span class="n">{choices.length}</span><ChevronDown size={12} />{/if}
    </button>
    <SourceControl {scm} onopen={(w) => (picked = w)} />
  {/if}
</section>

<style>
  .side-git {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
  }
  .empty {
    margin: 0;
    padding: 14px 12px;
    color: var(--text-3);
    font-size: 13px;
  }
  .wt {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 12px 0;
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }
  .wt:disabled {
    cursor: default;
  }
  .wt:hover:not(:disabled) {
    background: var(--surface-hover);
  }
  .nm {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .n {
    font-size: 11px;
    color: var(--text-3);
  }
</style>
