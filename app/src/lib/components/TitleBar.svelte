<script lang="ts">
  import Search from '@lucide/svelte/icons/search';
  import Columns2 from '@lucide/svelte/icons/columns-2';
  import PanelRight from '@lucide/svelte/icons/panel-right';
  import Play from '@lucide/svelte/icons/play';
  import Settings from '@lucide/svelte/icons/settings';
  import MessageSquarePlus from '@lucide/svelte/icons/message-square-plus';
  import { feedback, ft } from '../feedback/feedback.svelte';
  import { t } from '../i18n.svelte';
  import { command } from '../commands/commands';
  import { hint } from '../commands/keyboard';
  import { isMac as mac } from '../platform';
  import { app, type View } from '../state.svelte';
  import { closePaneAt, toggleSplit, update, workspace } from '../workspace/workspace.svelte';
  import HostChip from './titlebar/HostChip.svelte';
  import PortsChip from './ports/PortsChip.svelte';
  import NewSessionMenu from './titlebar/NewSessionMenu.svelte';
  import UpdatePill from './titlebar/UpdatePill.svelte';
  import { isDemo } from '../daemon/demo';
  import NotificationBell from './titlebar/NotificationBell.svelte';
  import { startable } from '../kanban/kanban-core';
  import { preload } from '../idle';

  const views: { id: View; label: () => string }[] = [
    { id: 'agents', label: () => t('title.agents') },
    { id: 'kanban', label: () => t('title.kanban') },
    { id: 'browser', label: () => t('title.browser') },
  ];
  const nextAuto = $derived(startable(app.data.tasks)[0] ?? null);

  async function run() {
    if (!nextAuto) return;
    try {
      await app.api.startTask(nextAuto.id);
    } catch (e) {
      app.notify(String((e as Error).message ?? e), 'error');
    }
  }
  const systemChip = preload(() => import('./system/SystemChip.svelte'));
  const usageChip = preload(() => import('./usage/UsageChip.svelte'));
  const demoPill = isDemo() ? () => import('./ui/DemoPill.svelte') : null;
</script>

<header class="bar" class:mac data-tauri-drag-region>
  <img class="logo" src="/logo.svg" alt={t('app.name')} width="18" height="18" data-tauri-drag-region />
  <div class="group">
    <button
      class="icon-btn"
      class:active={workspace.layout.panes.length > 1}
      title={t('title.layout')}
      aria-label={t('title.layout')}
      onclick={() => (workspace.layout.panes.length > 1 ? closePaneAt(1) : update(toggleSplit))}><Columns2 size={15} /></button
    >
    <NewSessionMenu />
    <button class="icon-btn" data-tour="palette" title={`${t('title.palette')} (${hint('palette')})`} aria-label={t('title.palette')} onclick={() => app.open('palette')}
      ><Search size={15} /></button
    >
  </div>
  <div class="views" role="tablist">
    {#each views as v (v.id)}
      <button role="tab" data-tour={v.id === 'kanban' ? 'kanban' : undefined} aria-selected={app.view === v.id} class:on={app.view === v.id} onclick={() => (app.view = v.id)}>{v.label()}</button>
    {/each}
  </div>
  <div class="spacer" data-tauri-drag-region></div>
  {#if demoPill}{#await demoPill() then { default: DemoPill }}<DemoPill />{/await}{/if}
  <UpdatePill />
  {#await systemChip() then { default: SystemChip }}<SystemChip />{/await}
  {#await usageChip() then { default: UsageChip }}<UsageChip />{/await}
  <PortsChip />
  <HostChip />
  <button class="btn primary run" disabled={!nextAuto} title={t('title.runHint')} onclick={run}>
    <Play size={12} />
    {t('title.run')}
  </button>
  <button
    class="icon-btn"
    class:active={app.panelOpen}
    title={t('title.panel')}
    aria-label={t('title.panel')}
    onclick={() => command('panel').run()}><PanelRight size={15} /></button
  >
  <NotificationBell />
  <button class="icon-btn" data-tour="feedback" title={ft('title')} aria-label={ft('title')} onclick={() => feedback.show()}><MessageSquarePlus size={15} /></button>
  <button class="icon-btn" data-tour="settings" title={t('title.settings')} aria-label={t('title.settings')} onclick={() => app.open('settings')}
    ><Settings size={15} /></button
  >
</header>

<style>
  .bar {
    height: var(--titlebar-h);
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 10px 0 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .bar.mac {
    padding-left: 78px;
  }
  .logo {
    border-radius: 4px;
    box-shadow: 0 0 0 1px var(--border-strong);
  }
  .group {
    display: flex;
    gap: 2px;
  }
  .views {
    display: flex;
    padding: 2px;
    gap: 2px;
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .views button {
    height: 24px;
    padding: 0 10px;
    border-radius: 4px;
    color: var(--text-2);
    font-weight: 500;
    transition:
      background var(--ease),
      color var(--ease);
  }
  .views button:hover {
    color: var(--text);
  }
  .views button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .run {
    height: 26px;
    font-size: 12px;
    letter-spacing: 0.03em;
  }
</style>
