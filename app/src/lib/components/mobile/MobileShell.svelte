<script lang="ts">
  import Menu from '@lucide/svelte/icons/menu';
  import X from '@lucide/svelte/icons/x';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import KanbanSquare from '@lucide/svelte/icons/square-kanban';
  import GitCompare from '@lucide/svelte/icons/git-compare';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import type { Component } from 'svelte';
  import { onMount, untrack } from 'svelte';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { needsInputCount } from '../../notify';
  import { isReviewKey, reviewTitle } from '../../review/open';
  import { MOBILE_TABS, type MobileTab } from '../../mobile/layout';
  import { nextTab, viewForTab, type NavState } from '../../mobile/nav';
  import { ui } from '../../mobile/ui.svelte';
  import Sidebar from '../sidebar/Sidebar.svelte';
  import Workspace from '../workspace/Workspace.svelte';
  import SidePanel from '../panel/SidePanel.svelte';
  import { preload } from '../../idle';
  import DemoPill from '../ui/DemoPill.svelte';
  import MobileChips from './MobileChips.svelte';
  import MobileReviews from './MobileReviews.svelte';
  import MobileMore from './MobileMore.svelte';
  import { apk, pillLabel } from '../../mobile/apk-update.svelte';
  import { backAction, backHandlers, closeTop, escapeCloses, pushTab, setBackRunner } from '../../mobile/back';
  import { native } from '../../mobile/native';
  import { diag } from '../../diag.svelte';
  import { tour } from '../../tour/tour.svelte';

  const kanban = () => import('../kanban/KanbanView.svelte');

  const NAV_TOUR: Partial<Record<MobileTab, string>> = { tasks: 'kanban', review: 'review', more: 'palette' };
  const icons: Record<MobileTab, Component<{ size?: number }>> = { agents: SquareTerminal, tasks: KanbanSquare, review: GitCompare, more: Ellipsis };
  const waiting = $derived(needsInputCount(app.data));
  const reviewOpen = $derived(isReviewKey(app.selectedKey));
  const stageVisible = $derived(ui.tab === 'agents' || (ui.tab === 'review' && reviewOpen));
  const connPhase = $derived(app.conn.phase);
  const title = $derived.by(() => {
    if (ui.tab === 'tasks') return t('mobile.nav.tasks');
    if (ui.tab === 'more') return t('mobile.nav.more');
    if (ui.tab === 'review') return reviewOpen && app.selectedKey ? (reviewTitle(app.selectedKey) ?? t('mobile.shell.reviews')) : t('mobile.shell.reviews');
    return app.selected?.title ?? app.currentWorkspace?.name ?? t('app.name');
  });
  const subtitle = $derived(ui.tab === 'agents' && app.selected ? (app.currentWorkspace?.name ?? '') : '');
  const updateLabel = $derived(__SELF_UPDATE__ && apk.enabled ? pillLabel(apk) : null);
  const hostLabel = $derived(app.conn.target.kind === 'local' ? t('host.local') : app.conn.target.label || t('mobile.more.notConnected'));

  let last: NavState = { view: app.view, key: app.selectedKey };
  $effect(() => {
    const now: NavState = { view: app.view, key: app.selectedKey };
    untrack(() => {
      ui.tab = nextTab(ui.tab, last, now);
      last = now;
    });
  });

  let drawerFrom: { key: string | null; ws: string } | null = null;
  $effect(() => {
    const open = ui.drawer;
    const key = app.selectedKey;
    const ws = app.workspaceId;
    untrack(() => {
      if (!open) return void (drawerFrom = null);
      if (!drawerFrom) return void (drawerFrom = { key, ws });
      if (drawerFrom.key !== key || drawerFrom.ws !== ws) ui.drawer = false;
    });
  });

  $effect(() => {
    if (!app.selected?.lineId) ui.panel = false;
  });

  function go(tab: MobileTab) {
    if (tab === 'review' && ui.tab === 'review' && reviewOpen) app.selectedKey = null;
    ui.tab = tab;
    app.view = viewForTab(tab, app.view);
    last = { view: app.view, key: app.selectedKey };
    ui.panel = false;
  }

  function back() {
    app.selectedKey = null;
    last = { view: app.view, key: null };
  }

  let history: MobileTab[] = [];
  let shown: MobileTab = untrack(() => ui.tab);
  let returning = false;
  $effect(() => {
    const tab = ui.tab;
    untrack(() => {
      if (tab !== shown && !returning) history = pushTab(history, shown, tab);
      returning = false;
      shown = tab;
    });
  });

  function goBack() {
    const previousTab = history.at(-1) ?? null;
    const action = backAction({
      handlers: backHandlers(),
      tour: !!tour.phase,
      menu: !!app.menu,
      diag: !!diag.mode,
      overlay: !!app.overlay,
      entry: app.overlay === 'connect' && app.conn.phase === 'idle',
      panel: ui.panel,
      drawer: ui.drawer,
      subscreen: ui.tab === 'review' && reviewOpen,
      previousTab,
    });
    if (action === 'tour') tour.finish();
    else if (action === 'menu') app.menu = null;
    else if (action === 'handler') closeTop();
    else if (action === 'diag') diag.close();
    else if (action === 'overlay') app.closeOverlay();
    else if (action === 'panel') ui.panel = false;
    else if (action === 'drawer') ui.drawer = false;
    else if (action === 'subscreen') back();
    else if (action === 'tab' && previousTab) {
      history = history.slice(0, -1);
      returning = previousTab !== ui.tab;
      go(previousTab);
    } else native.minimize().catch(() => {});
  }

  onMount(() => {
    setBackRunner(goBack);
    tour.setReady(true);
    return () => tour.setReady(false);
  });
  const connBanner = preload(() => import('../ui/ConnBanner.svelte'), 1000);
</script>

<div class="mshell">
  <header class="top">
    {#if ui.tab === 'review' && reviewOpen}
      <button class="icon-btn" aria-label={t('mobile.shell.back')} onclick={back}><ArrowLeft size={20} /></button>
    {:else}
      <button class="icon-btn" data-tour="workspaces" aria-label={t('mobile.shell.menu')} aria-expanded={ui.drawer} onclick={() => (ui.drawer = true)}><Menu size={20} /></button>
    {/if}
    <div class="titles">
      <span class="title">{title}</span>
      {#if subtitle}<span class="subtitle faint">{subtitle}</span>{/if}
    </div>
    {#if __SELF_UPDATE__ && updateLabel}
      <button class="upd" class:busy={apk.busy} onclick={() => (apk.sheet = true)}><span class="udot"></span>{updateLabel}</button>
    {/if}
    <DemoPill compact />
    <button class="host" data-tour="host" onclick={() => app.open('connect')} aria-label={t('host.connect')}>
      <span class="dot" style:background={connPhase === 'connected' ? 'var(--st-input)' : connPhase === 'error' ? 'var(--st-failed)' : connPhase === 'idle' ? 'var(--st-held)' : 'var(--st-working)'}></span>
      <span class="hl">{hostLabel}</span>
    </button>
  </header>
  {#await connBanner() then { default: ConnBanner }}<ConnBanner />{/await}

  <main class="content">
    <div class="stage" class:hidden={!stageVisible} aria-hidden={!stageVisible}>
      {#if ui.tab !== 'review'}<MobileChips />{/if}
      <div class="ws">
        <Workspace />
      </div>
    </div>
    {#if ui.tab === 'tasks'}
      <div class="screen">
        {#await kanban() then { default: KanbanView }}<KanbanView />{/await}
      </div>
    {:else if ui.tab === 'review' && !reviewOpen}
      <div class="screen"><MobileReviews /></div>
    {:else if ui.tab === 'more'}
      <div class="screen"><MobileMore /></div>
    {/if}
  </main>

  <nav class="bottom" aria-label={t('mobile.nav.label')}>
    {#each MOBILE_TABS as tab (tab)}
      {@const Icon = icons[tab]}
      <button class:on={ui.tab === tab} data-tour={NAV_TOUR[tab]} aria-current={ui.tab === tab ? 'page' : undefined} onclick={() => go(tab)}>
        <span class="ic">
          <Icon size={20} />
          {#if tab === 'agents' && waiting > 0}<span class="badge">{waiting}</span>{/if}
        </span>
        <span class="lb">{t(`mobile.nav.${tab}`)}</span>
      </button>
    {/each}
  </nav>
</div>

{#if ui.drawer}
  <div class="scrim" role="presentation" onclick={() => (ui.drawer = false)}></div>
  <div class="drawer" role="dialog" aria-modal="true" aria-label={t('mobile.shell.menu')}>
    <div class="drawer-head">
      <span class="title">{t('mobile.shell.menu')}</span>
      <button class="icon-btn" aria-label={t('common.close')} onclick={() => (ui.drawer = false)}><X size={20} /></button>
    </div>
    <Sidebar />
  </div>
{/if}

<svelte:window onkeydown={(e) => escapeCloses(e, ui.panel) && (ui.panel = false)} />

{#if ui.panel && app.selected?.lineId}
  <div class="sheet" role="dialog" aria-modal="true" aria-label={t('mobile.shell.activity')}>
    <div class="drawer-head">
      <span class="title">{t('mobile.shell.activity')}</span>
      <button class="icon-btn" aria-label={t('common.close')} onclick={() => (ui.panel = false)}><X size={20} /></button>
    </div>
    <SidePanel />
  </div>
{/if}

<style>
  .mshell {
    display: flex;
    flex-direction: column;
    height: calc(100% - var(--kb, 0px));
    padding-left: var(--safe-left);
    padding-right: var(--safe-right);
    background: var(--bg);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: calc(var(--safe-top) + 4px) 8px 4px 4px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .titles {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.25;
  }
  .title {
    font-weight: 600;
    font-size: 16px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .subtitle {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .upd {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    min-height: 36px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 18px;
    background: var(--surface);
    color: var(--text);
    font-size: 12.5px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .udot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .upd.busy .udot {
    animation: upd-pulse 1.2s ease-in-out infinite;
  }
  @keyframes upd-pulse {
    50% {
      opacity: 0.3;
    }
  }
  .host {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 40%;
    min-height: 36px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 18px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 13px;
  }
  .hl {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .content {
    flex: 1;
    min-height: 0;
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .stage {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--term-bg);
  }
  .stage.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .ws {
    flex: 1;
    min-height: 0;
    display: flex;
    position: relative;
  }
  .ws :global(.tabbar),
  .ws :global(.divider) {
    display: none;
  }
  .ws :global(.slot) {
    flex-basis: 100% !important;
  }
  .ws :global(.slot:not(:has(.pane.focused))) {
    display: none;
  }
  .screen {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .bottom {
    display: flex;
    border-top: 1px solid var(--border);
    background: var(--bg-subtle);
    padding-bottom: var(--safe-bottom);
  }
  :global(:root.kb) .bottom {
    display: none;
  }
  .bottom button {
    flex: 1;
    min-height: 56px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    color: var(--text-3);
    font-size: 11px;
    font-weight: 500;
  }
  .bottom button.on {
    color: var(--accent);
  }
  .ic {
    position: relative;
    display: grid;
    place-items: center;
  }
  .badge {
    position: absolute;
    top: -5px;
    left: 13px;
    min-width: 17px;
    height: 17px;
    padding: 0 4px;
    border-radius: 9px;
    background: var(--st-input);
    color: #fff;
    font-size: 10.5px;
    font-weight: 600;
    line-height: 17px;
    text-align: center;
  }
  .scrim {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    z-index: 40;
  }
  .drawer {
    position: fixed;
    top: 0;
    bottom: 0;
    left: 0;
    width: min(86vw, 360px);
    z-index: 41;
    display: flex;
    flex-direction: column;
    padding: var(--safe-top) 0 var(--safe-bottom) var(--safe-left);
    background: var(--bg-subtle);
    box-shadow: var(--shadow);
    animation: slide-in 180ms cubic-bezier(0.2, 0, 0, 1);
  }
  .drawer :global(.sidebar) {
    width: 100%;
    flex: 1;
    min-height: 0;
    border-right: 0;
  }
  .drawer-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 4px 4px 16px;
    border-bottom: 1px solid var(--border);
  }
  .sheet {
    position: fixed;
    inset: 0;
    z-index: 41;
    display: flex;
    flex-direction: column;
    padding: var(--safe-top) var(--safe-right) var(--safe-bottom) var(--safe-left);
    background: var(--bg-subtle);
    animation: rise 180ms cubic-bezier(0.2, 0, 0, 1);
  }
  .sheet :global(.panel) {
    width: 100%;
    flex: 1;
    min-height: 0;
    border-left: 0;
  }
  @keyframes slide-in {
    from {
      transform: translateX(-100%);
    }
  }
  @keyframes rise {
    from {
      transform: translateY(100%);
    }
  }
</style>
