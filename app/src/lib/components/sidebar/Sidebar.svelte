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
  import { command } from '../../commands/commands';
  import { hint } from '../../commands/keyboard';
  import { touchContextMenu } from '../../mobile/touch-menu';
  import { app, type SidebarMode } from '../../state.svelte';
  import { broadcastTo, groupPicker } from '../../workspace/wsactions';
  import WandSparkles from '@lucide/svelte/icons/wand-sparkles';
  import { saveSettings, settings } from '../../settings/settings.svelte';
  import { organizer } from '../../workspace/organize.svelte';
  import { nudgeVisible, unplacedCount } from '../../workspace/organize';
  import SpacesView from './SpacesView.svelte';
  import { preload } from '../../idle';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import { isPhone } from '../../mobile/ui.svelte';
  import { notesUi } from '../../notes.svelte';

  const statesView = preload(() => import('./StatesView.svelte'));
  // Only windows showing several hosts need it.
  const hostSections = () => import('./HostSections.svelte');
  const gitView = () => import('../../git/SidebarGit.svelte');
  const notesView = () => import('./NotesPopover.svelte');

  const unplaced = $derived(app.connected && app.wsPersistent ? unplacedCount(app.data.lines, app.ws.workspaces, app.projects, app.data.tasks) : 0);
  const nudge = $derived(nudgeVisible(unplaced, settings.organizeNudgeAt));
  let filter = $state('');
  let showArchived = $state(false);
  const modes: { id: SidebarMode; label: () => string }[] = [
    { id: 'workspaces', label: () => t('ws.title') },
    { id: 'status', label: () => t('ws.byStatus') },
  ];
  const phone = $derived(isPhone());
  const listed = $derived(app.items.filter((i) => i.kind !== 'shell').length);
  const selection = $derived([...app.multi]);
  const addMenu = () => [
    { label: t('ws.new'), icon: Plus, run: () => command('ws-new').run() },
    { label: t('ws.rescan'), icon: RefreshCw, run: () => command('rescan-folders').run() },
  ];
</script>

{#snippet wsActions()}
    <button class="icon-btn" title={t('ws.new')} aria-label={t('ws.new')} disabled={!command('ws-new').available()} aria-haspopup="menu" onclick={(e) => app.openMenu(e, t('ws.new'), addMenu())}
      ><Plus size={14} /></button
    >
    <button class="icon-btn" title={t('ws.newFolder')} aria-label={t('ws.newFolder')} disabled={!command('folder-new').available()} onclick={() => command('folder-new').run()}
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
{/snippet}

<aside class="sidebar" data-tour="workspaces" use:touchContextMenu>
  <header>
    <div class="modes" role="tablist" aria-label={t('ws.mode')}>
      {#each modes as m (m.id)}
        <button role="tab" title={m.label()} aria-selected={!app.sidebarGit && app.sidebarMode === m.id} class:on={!app.sidebarGit && app.sidebarMode === m.id} onclick={() => app.setSidebarMode(m.id)}>{m.label()}</button>
      {/each}
      <button role="tab" class="git" title={`${t('git.scm')} (${hint('sourceControl')})`} aria-label={t('git.scm')} aria-selected={app.sidebarGit} class:on={app.sidebarGit} data-tour="sidebar-git-tab" onclick={() => app.setSidebarGit(true)}><GitBranch size={13} />{t('git.side.tab')}</button>
    </div>
    {#if app.sidebarGit}
      <span></span>
    {:else if app.sidebarMode === 'workspaces'}
      {#if !phone}{@render wsActions()}{/if}
    {:else}
      <span class="count faint">{listed}</span>
    {/if}
  </header>
  {#if !app.sidebarGit}
  <div class="tools">
    <label class="filter">
      <Search size={13} />
      <input bind:value={filter} placeholder={app.sidebarMode === 'workspaces' ? t('ws.filter') : t('sidebar.filter')} aria-label={t('sidebar.filter')} />
    </label>
    {#if phone && app.sidebarMode === 'workspaces'}{@render wsActions()}{/if}
  </div>
  {#if nudge}
    <div class="nudge" data-tour="organize">
      <WandSparkles size={14} />
      <span class="msg">{t('org.nudge', { n: unplaced })}</span>
      <button class="btn" onclick={() => organizer.show()}>{t('org.nudgeAction')}</button>
      <button class="icon-btn" title={t('org.nudgeHide')} aria-label={t('org.nudgeHide')} onclick={() => saveSettings({ organizeNudgeAt: unplaced })}><X size={13} /></button>
    </div>
  {/if}
  {/if}
  {#if app.sidebarGit}
    {#await gitView() then { default: SidebarGit }}<SidebarGit />{/await}
  {:else if app.hosts.length > 1}
    {#await hostSections() then { default: HostSections }}<HostSections {filter} {showArchived} />{/await}
  {:else if app.sidebarMode === 'workspaces'}
    <SpacesView {filter} {showArchived} />
  {:else}
    {#await statesView() then { default: StatesView }}<StatesView {filter} />{/await}
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
  {#if notesUi.lineId}
    {#await notesView() then { default: NotesPopover }}<NotesPopover />{/await}
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
    -webkit-touch-callout: none;
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
  .modes button.git {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 8px;
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
  .tools {
    display: contents;
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
  :global(:root.phone) header {
    padding: 10px 12px 8px;
  }
  :global(:root.phone) .modes {
    margin-right: 0;
    padding: 3px;
    border-radius: 10px;
  }
  :global(:root.phone) .modes button {
    flex: 1 1 0;
    height: 40px;
    border-radius: 8px;
    font-size: 14px;
  }
  :global(:root.phone) .modes button.git {
    flex: 1 1 0;
    justify-content: center;
    gap: 6px;
  }
  :global(:root.phone) .tools {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0 8px 8px 12px;
  }
  :global(:root.phone) .filter {
    flex: 1;
    min-width: 0;
    margin: 0 4px 0 0;
    border-radius: 10px;
  }
  :global(:root.phone) .filter input {
    padding: 0;
  }

</style>
