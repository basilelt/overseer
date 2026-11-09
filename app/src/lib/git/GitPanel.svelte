<script lang="ts">
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import X from '@lucide/svelte/icons/x';
  import { app } from '../state.svelte';
  import { itemTarget } from '../review/open';
  import { gitApi } from './api';
  import { Scm } from './scm.svelte';
  import SourceControl from './SourceControl.svelte';
  import { gt } from './strings';

  const scm = new Scm(
    () => (app.client ? gitApi(app.client) : null),
    (text) => app.notify(text, 'error'),
  );
  const target = $derived(itemTarget(app.selectedKey));

  $effect(() => {
    scm.setTarget(target);
  });

  function expand() {
    const s = scm.status;
    if (!s) return;
    app.gitWorktree = { project: s.project, path: s.worktree };
    app.openProjectTab('graph', s.project);
  }
</script>

<aside class="panel" aria-label={gt('button')} data-tour="git-panel">
  <header>
    <span class="title">{gt('button')}</span>
    <button class="icon-btn" title={gt('expand')} aria-label={gt('expand')} onclick={expand} disabled={!scm.status}><Maximize2 size={13} /></button>
    <button class="icon-btn" title={gt('closePanel')} aria-label={gt('closePanel')} onclick={() => (app.gitPanel = false)}><X size={14} /></button>
  </header>
  {#if target}
    <SourceControl {scm} />
  {:else}
    <p class="faint empty">{gt('noTarget')}</p>
  {/if}
</aside>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg);
    border-left: 1px solid var(--border);
  }
  header {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 40px;
    flex: none;
    padding: 0 8px 0 14px;
    border-bottom: 1px solid var(--border);
  }
  .title {
    flex: 1;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .empty {
    margin: 0;
    padding: 14px;
    font-size: 13px;
  }
</style>
