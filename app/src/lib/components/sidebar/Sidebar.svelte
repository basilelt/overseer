<script lang="ts">
  import Archive from '@lucide/svelte/icons/archive';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import Megaphone from '@lucide/svelte/icons/megaphone';
  import Plus from '@lucide/svelte/icons/plus';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Search from '@lucide/svelte/icons/search';
  import Users from '@lucide/svelte/icons/users';
  import X from '@lucide/svelte/icons/x';
  import { t } from '../../i18n.svelte';
  import { app, type SidebarMode } from '../../state.svelte';
  import { broadcastTo, createFolder, groupPicker, rescanFolders } from '../../wsactions';
  import WandSparkles from '@lucide/svelte/icons/wand-sparkles';
  import { saveSettings, settings } from '../../settings.svelte';
  import { organizer } from '../../organize.svelte';
  import { nudgeVisible, unplacedCount } from '../../organize';
  import LayoutList from '@lucide/svelte/icons/layout-list';
  import ListTree from '@lucide/svelte/icons/list-tree';
  import SpacesView from './SpacesView.svelte';
  import StatusList from './StatusList.svelte';
  import WorkspaceTree from './WorkspaceTree.svelte';

  const unplaced = $derived(app.connected && app.wsPersistent ? unplacedCount(app.data.lines, app.ws.workspaces, app.projects, app.data.tasks) : 0);
  const nudge = $derived(nudgeVisible(unplaced, settings.organizeNudgeAt));
  let filter = $state('');
  let showArchived = $state(false);
  // The compact view is the default; the folder tree stays one click away until it is retired.
  const compact = $derived((settings.workspacesLayout ?? 'spaces') === 'spaces');
  const modes: { id: SidebarMode; label: () => string }[] = [
    { id: 'workspaces', label: () => t('ws.title') },
    { id: 'status', label: () => t('ws.byStatus') },
  ];
  const selection = $derived([...app.multi]);
  const addMenu = () => [
    { label: t('ws.new'), icon: Plus, run: () => app.open('workspaceNew') },
    { label: t('ws.rescan'), icon: RefreshCw, run: () => rescanFolders() },
  ];
</script>

<aside class="sidebar" data-tour="workspaces">
  <header>
    <div class="modes" role="tablist" aria-label={t('ws.mode')}>
      {#each modes as m (m.id)}
        <button role="tab" title={m.label()} aria-selected={app.sidebarMode === m.id} class:on={app.sidebarMode === m.id} onclick={() => app.setSidebarMode(m.id)}>{m.label()}</button>
      {/each}
    </div>
    {#if app.sidebarMode === 'workspaces'}
      <button
        class="icon-btn"
        title={compact ? t('ws.layoutTree') : t('ws.layoutSpaces')}
        aria-label={compact ? t('ws.layoutTree') : t('ws.layoutSpaces')}
        onclick={() => saveSettings({ workspacesLayout: compact ? 'tree' : 'spaces' })}
        >{#if compact}<ListTree size={14} />{:else}<LayoutList size={14} />{/if}</button
      >
      <button class="icon-btn" title={t('ws.new')} aria-label={t('ws.new')} disabled={!app.connected || !app.wsPersistent} aria-haspopup="menu" onclick={(e) => app.openMenu(e, t('ws.new'), addMenu())}
        ><Plus size={14} /></button
      >
      <button class="icon-btn" title={t('ws.newFolder')} aria-label={t('ws.newFolder')} disabled={!app.connected || !app.wsPersistent} onclick={() => createFolder()}
        ><FolderPlus size={14} /></button
      >
      <button
        class="icon-btn"
        class:active={showArchived}
        title={t('ws.showArchived')}
        aria-label={t('ws.showArchived')}
        aria-pressed={showArchived}
        onclick={() => (showArchived = !showArchived)}><Archive size={14} /></button
      >
    {:else}
      <span class="count faint">{app.items.length}</span>
    {/if}
  </header>
  <label class="filter">
    <Search size={13} />
    <input bind:value={filter} placeholder={app.sidebarMode === 'workspaces' ? t('ws.filter') : t('sidebar.filter')} aria-label={t('sidebar.filter')} />
  </label>
  {#if nudge}
    <div class="nudge" data-tour="organize">
      <WandSparkles size={14} />
      <span class="msg">{t('org.nudge', { n: unplaced })}</span>
      <button class="btn" onclick={() => organizer.show()}>{t('org.nudgeAction')}</button>
      <button class="icon-btn" title={t('org.nudgeHide')} aria-label={t('org.nudgeHide')} onclick={() => saveSettings({ organizeNudgeAt: unplaced })}><X size={13} /></button>
    </div>
  {/if}
  {#if app.sidebarMode === 'workspaces'}
    {#if compact}<SpacesView {filter} />{:else}<WorkspaceTree {filter} {showArchived} />{/if}
  {:else}
    <StatusList {filter} />
  {/if}
  {#if selection.length > 1}
    <div class="selbar" role="toolbar" aria-label={t('ws.selection', { n: selection.length })}>
      <span class="n">{t('ws.selection', { n: selection.length })}</span>
      <button class="icon-btn" title={t('broadcast.title')} aria-label={t('broadcast.title')} onclick={() => broadcastTo({ kind: 'items', keys: selection })}
        ><Megaphone size={14} /></button
      >
      <button class="icon-btn" title={t('grp.addSelection')} aria-label={t('grp.addSelection')} onclick={(e) => app.openMenu(e, t('grp.addSelection'), groupPicker(selection))}
        ><Users size={14} /></button
      >
      <button class="icon-btn" title={t('common.close')} aria-label={t('common.close')} onclick={() => (app.multi = new Set())}><X size={14} /></button>
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    width: var(--sidebar-w, 288px);
    flex: none;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    background: var(--bg-subtle);
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 10px 10px 8px;
  }
  .modes {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 2px;
    padding: 2px;
    margin-right: 6px;
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .modes button {
    flex: 1 1 auto;
    min-width: 0;
    padding: 0 4px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    height: 24px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
    transition:
      background var(--ease),
      color var(--ease);
  }
  .modes button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
  .count {
    font-size: 12px;
    padding: 0 6px;
    font-variant-numeric: tabular-nums;
  }
  .filter {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 10px 6px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-3);
  }
  .filter:focus-within {
    border-color: var(--accent);
  }
  .filter input {
    flex: 1;
    border: 0;
    padding: 5px 0;
    background: transparent;
    min-width: 0;
  }
  .nudge {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 10px 6px;
    padding: 6px 4px 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-2);
    font-size: 12px;
  }
  .nudge .msg {
    flex: 1;
    min-width: 0;
    line-height: 1.3;
  }
  .nudge .btn {
    flex: none;
    padding: 2px 8px;
    font-size: 12px;
  }
  .selbar {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0 8px 8px;
    padding: 4px 4px 4px 10px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .n {
    flex: 1;
    font-size: 12px;
    font-weight: 500;
  }
</style>
