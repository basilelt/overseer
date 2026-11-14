<script lang="ts">
  import Play from '@lucide/svelte/icons/play';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import X from '@lucide/svelte/icons/x';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { organizer } from '../../workspace/organize.svelte';
  import type { PickUpItem } from '../../workspace/organize';

  let { items }: { items: PickUpItem[] } = $props();

  let busy = $state<string | null>(null);
  const shown = $derived(items.filter((i) => !organizer.hidden.includes(i.lineId)));
  const titleOf = (i: PickUpItem) => i.title || app.data.lines.find((l) => l.line.id === i.lineId)?.local?.title || i.branch || i.lineId;
  const wsOf = (i: PickUpItem) => i.workspaceId ?? app.itemWs.get(`line:${i.lineId}`);

  async function resume(i: PickUpItem) {
    busy = i.lineId;
    try {
      const term = await app.api.resume(i.lineId);
      app.upsertTerm(term);
      organizer.close();
      app.select(`line:${i.lineId}`);
    } catch (e) {
      app.notify(t('org.failed', { error: String((e as Error)?.message ?? e) }), 'error');
    } finally {
      busy = null;
    }
  }

  function openWs(i: PickUpItem) {
    const ws = wsOf(i);
    organizer.close();
    if (ws) app.selectWorkspace(ws);
    app.select(`line:${i.lineId}`);
  }
</script>

<Modal title={t('org.pickUp')} onclose={() => organizer.close()} width={640}>
  {#if !shown.length}
    <p class="faint">{t('org.nothingPick')}</p>
  {/if}
  <ul class="cards">
    {#each shown as i (i.lineId)}
      <li class="card">
        <div class="head">
          <span class="title">{titleOf(i)}</span>
          <span class="badge {i.why}">{t(`org.why.${i.why}` as Key)}</span>
          <button class="icon-btn" title={t('org.dismiss')} aria-label={t('org.dismiss')} onclick={() => organizer.hide(i.lineId)}><X size={14} /></button>
        </div>
        {#if i.summary}<p class="sum">{i.summary}</p>{/if}
        <div class="meta faint">
          {#if i.branch}<span class="branch"><GitBranch size={12} />{i.branch}</span>{/if}
          {#if i.edited}<span>{t('org.edited', { n: i.edited })}</span>{/if}
          {#if i.todos}<span>{t('org.todos', { n: i.todos })}</span>{/if}
        </div>
        <div class="acts">
          <button class="btn primary" disabled={busy === i.lineId} onclick={() => void resume(i)}><Play size={13} />{t('org.resume')}</button>
          <button class="btn" onclick={() => openWs(i)}><FolderOpen size={13} />{t('org.openWs')}</button>
        </div>
      </li>
    {/each}
  </ul>
  {#snippet footer()}
    <span class="spacer"></span>
    <button class="btn" onclick={() => organizer.close()}>{t('common.close')}</button>
  {/snippet}
</Modal>

<style>
  .cards {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .card {
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .title {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    flex: none;
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 11px;
    background: var(--surface-2);
    color: var(--text-2);
  }
  .badge.dirty {
    color: var(--text);
  }
  .badge.todos {
    color: var(--accent);
  }
  .sum {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    font-size: 11px;
  }
  .branch {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--mono, monospace);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .acts .btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
</style>
