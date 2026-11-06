<script lang="ts">
  import { onMount } from 'svelte';
  import { attention } from '../../attention';
  import { t } from '../../i18n.svelte';
  import { saveSettings, settings } from '../../settings.svelte';
  import { hostFor, spaces, toggleOpen } from '../../spaces';
  import { hostKey } from '../../window';
  import { visibleClock } from '../../visible-clock';
  import { app } from '../../state.svelte';
  import { workspaceMenu } from '../../wsactions';
  import AttentionBanner from './AttentionBanner.svelte';
  import SpaceBlock from './SpaceBlock.svelte';

  let { filter }: { filter: string } = $props();

  let now = $state(Date.now());
  let closedOpen = $state(new Set<string>());

  // One clock for the whole view, paused while the window is hidden.
  onMount(() => visibleClock((n) => (now = n)));

  const view = $derived(spaces(app.ws.workspaces, app.items, app.itemWs, filter));
  const banner = $derived(attention(app.items, app.itemWs));
  const hk = $derived(hostKey(app.conn.target));
  const saved = $derived(settings.spacesOpen?.[hk]);
  const opened = $derived(new Set(saved ?? [app.workspaceId]));
  // Every session of a connection runs on that connection's host.
  const host = $derived(hostFor(app.conn));
  const q = $derived(filter.trim());

  function toggle(id: string) {
    saveSettings({ spacesOpen: { ...settings.spacesOpen, [hostKey(app.conn.target)]: toggleOpen(saved, id, new Set(['', ...app.ws.workspaces.map((w) => w.id)]), app.workspaceId) } });
  }

  function toggleClosed(id: string) {
    const next = new Set(closedOpen);
    if (!next.delete(id)) next.add(id);
    closedOpen = next;
  }
</script>

<div class="spaces">
  <AttentionBanner entries={banner} onselect={(e) => app.select(e.key)} />
  {#each view.rows as r (r.id)}
    <SpaceBlock
      row={r}
      {now}
      {host}
      open={opened.has(r.id)}
      closedOpen={closedOpen.has(r.id)}
      selectedKey={app.selectedKey}
      ontoggle={() => toggle(r.id)}
      onclosed={() => toggleClosed(r.id)}
      onselect={(key) => app.select(key)}
      onmenu={r.workspace ? (e) => app.openMenu(e, r.workspace!.name, workspaceMenu(r.workspace!)) : undefined}
    />
  {/each}
  {#if view.empty.length}
    <div class="empty">
      <span class="label">{t('ws.noSession')}</span>
      <div class="chips">
        {#each view.empty as w (w.id)}
          <button class="chip" type="button" onclick={() => app.selectWorkspace(w.id)}>{w.name}</button>
        {/each}
      </div>
    </div>
  {/if}
  {#if !view.rows.length && !view.empty.length}<p class="none faint">{q ? t('common.none') : t('ws.empty')}</p>{/if}
</div>

<style>
  .spaces {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 0 8px;
  }
  .empty {
    margin: 0 6px;
    padding: 0 6px;
  }
  .label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
  }
  .chip {
    max-width: 100%;
    height: 22px;
    padding: 0 8px;
    border: 1px dashed var(--ws-frame-border);
    border-radius: 11px;
    background: none;
    color: var(--text-2);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip:hover {
    background: var(--surface-hover);
  }
  .none {
    padding: 24px 12px;
    text-align: center;
  }
</style>
