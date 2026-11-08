<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { app } from './lib/state.svelte';
  import TitleBar from './lib/components/TitleBar.svelte';
  import Sidebar from './lib/components/sidebar/Sidebar.svelte';
  import Workspace from './lib/components/workspace/Workspace.svelte';
  import SidePanel from './lib/components/panel/SidePanel.svelte';
  import Overlays from './lib/components/overlays/Overlays.svelte';
  import Toast from './lib/components/ui/Toast.svelte';
  import { offerSwitchboard, switchboard } from './lib/switchboard.svelte';
  import { preload } from './lib/idle';
  import { startNotifications } from './lib/notify';
  import { startPorts } from './lib/ports.svelte';
  import { startCenter } from './lib/notifications.svelte';
  import { followReview, openReviewFromHash } from './lib/review/open';
  import { isTauri } from './lib/client';
  import { themeReport } from './lib/terminal';
  import { colorPalette, customColors, resolvedTheme, saveSettings, setSystemDark, settings } from './lib/settings.svelte';
  import Resizer from './lib/components/ui/Resizer.svelte';
  import Boundary from './lib/components/ui/Boundary.svelte';
  import { PANEL, SIDEBAR, clampWidth } from './lib/resize';
  import { t } from './lib/i18n.svelte';
  import { startUpdater } from './lib/updater.svelte';
  import { isMainWindow } from './lib/window';
  import { startPresence } from './lib/presence';
  import { startRest } from './lib/rest';
  import { startShortcuts } from './lib/keyboard';
  import { startTextDefaults } from './lib/textinput';
  import { onResume, startLayout, ui } from './lib/mobile/ui.svelte';
  import { diag, startDiagnostics } from './lib/diag.svelte';
  import { feedback } from './lib/feedback.svelte';
  import { tour } from './lib/tour/tour.svelte';
  import { isNativeAndroid, native, setNativeOs } from './lib/mobile/native';
  import { closeTop, runBack } from './lib/mobile/back';
  import { isWebLink } from './lib/links';

  let sidebarW = $state(clampWidth(settings.sidebarWidth, SIDEBAR));
  let panelW = $state(clampWidth(settings.panelWidth, PANEL));

  let shownAgents = $state(app.view !== 'kanban');
  $effect(() => {
    if (app.view !== 'kanban') shownAgents = true;
  });

  let nativeBars = $state(false);
  $effect(() => {
    const dark = resolvedTheme() === 'dark';
    if (nativeBars) native.systemBars(dark).catch(() => {});
  });
  $effect(() => {
    const theme = resolvedTheme();
    const report = themeReport(theme, customColors(), colorPalette());
    if (app.connected && app.conn.phase === 'connected') app.client?.request('POST', '/v1/theme', report).catch(() => {});
  });

  $effect(() => {
    const key = app.selectedKey;
    untrack(() => followReview(key));
  });

  const kanban = () => import('./lib/components/kanban/KanbanView.svelte');
  const tourView = () => import('./lib/components/tour/Tour.svelte');

  $effect(() => {
    tour.setReady(ui.layout !== 'phone');
  });
  $effect(() => {
    if (tour.ready && app.connected && switchboard.checked && !app.overlay && !feedback.open && !diag.mode && !ui.drawer && tour.pending()) tour.offer();
  });
  $effect(() => {
    if (app.connected && app.conn.phase === 'connected') untrack(() => void offerSwitchboard());
  });
  const browser = () => import('./lib/components/browser/BrowserView.svelte');
  const mobileShell = () => import('./lib/components/mobile/MobileShell.svelte');
  const apkHost = () => import('./lib/components/mobile/ApkUpdateHost.svelte');

  const viewOf = () => [app.view, app.overlay, app.selectedKey?.split(':')[0], app.conn.phase].filter(Boolean).join('/');

  onMount(() => {
    const stopDiag = startDiagnostics(viewOf);
    let stopLayout = startLayout();
    app.init().then(() => {
      if (app.mobile && !ui.mobileOs) {
        stopLayout();
        stopLayout = startLayout(true);
      }
      openReviewFromHash();
      if (app.mobile && __SELF_UPDATE__) void import('./lib/mobile/apk-update.svelte').then((m) => stops.push(m.startApkUpdater(app.os, onResume)));
    });
    const stops = [startPresence(), startTextDefaults(), stopDiag, startCenter(app), startNotifications(app), startPorts(app), onResume(() => app.wake()), () => stopLayout()];
    if (!ui.mobileOs && isMainWindow()) stops.push(startUpdater());
    stops.push(startShortcuts());
    stops.push(startRest());
    if (isTauri()) followSystemTheme().then((u) => stops.push(u));
    return () => stops.forEach((s) => s());
  });

  async function followSystemTheme(): Promise<() => void> {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const platform = await invoke<{ os: string; mobile: boolean }>('app_platform');
      if (platform.mobile) return followMobileSystem(platform.os);
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const w = getCurrentWindow();
      const theme = await w.theme();
      if (theme) setSystemDark(theme === 'dark');
      return await w.onThemeChanged(({ payload }) => setSystemDark(payload === 'dark'));
    } catch {
      return () => {};
    }
  }

  async function followMobileSystem(os: string): Promise<() => void> {
    setNativeOs(os);
    if (!isNativeAndroid()) return () => {};
    nativeBars = true;
    const dark = await native.systemDark().catch(() => null);
    if (typeof dark === 'boolean') setSystemDark(dark);
    const { onBackButtonPress } = await import('@tauri-apps/api/app');
    const listener = await onBackButtonPress(() => {
      if (!runBack() && !closeTop()) native.minimize().catch(() => {});
    });
    return () => void listener.unregister();
  }

  function onLinkMenu(e: MouseEvent) {
    const a = (e.target as Element | null)?.closest?.('a[href]') as HTMLAnchorElement | null;
    if (e.defaultPrevented || !a || !isWebLink(a.href)) return;
    e.preventDefault();
    app.linkMenu(e, a.href);
  }

  function onKey(e: KeyboardEvent) {
    if (tour.phase) return;
    const mod = (e.metaKey || e.ctrlKey) && (app.mobile || ui.layout === 'phone');
    if (mod && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      app.open(app.overlay === 'palette' ? null : 'palette');
    } else if (mod && e.key === ',') {
      e.preventDefault();
      app.open('settings');
    } else if (e.key === 'Escape' && diag.mode) {
      diag.close();
    } else if (e.key === 'Escape' && app.overlay) {
      app.open(null);
    }
  }
  const connBanner = preload(() => import('./lib/components/ui/ConnBanner.svelte'), 1000);
</script>

<svelte:window onkeydown={onKey} oncontextmenu={onLinkMenu} />

{#if ui.layout === 'phone'}
  <Boundary name="mobile">{#await mobileShell() then { default: MobileShell }}<MobileShell />{/await}</Boundary>
{:else}
<div class="shell">
  <TitleBar />
  {#await connBanner() then { default: ConnBanner }}<ConnBanner />{/await}
  <div class="body" style:--sidebar-w="min({sidebarW}px, 35vw)" style:--panel-w="min({panelW}px, 45vw)">
    {#if app.view === 'kanban'}
      <Boundary name="kanban">{#await kanban() then { default: KanbanView }}<KanbanView />{/await}</Boundary>
    {/if}
    {#if shownAgents}
    <div class="views" class:hidden={app.view === 'kanban'}>
      {#if app.sidebarOpen}
        <Boundary name="sidebar" width="var(--sidebar-w, 288px)"><Sidebar /></Boundary>
        <Resizer width={sidebarW} bounds={SIDEBAR} side="left" label={t('layout.resizeSidebar')} onchange={(w) => (sidebarW = w)} oncommit={(w) => saveSettings({ sidebarWidth: w })} />
      {/if}
      <main>
        <Boundary name="workspace">
          {#if app.view === 'browser'}
            {#await browser() then { default: BrowserView }}<BrowserView />{/await}
          {:else}
            <Workspace />
          {/if}
        </Boundary>
      </main>
      {#if app.panelOpen && app.view === 'agents' && !/^(graph|worktrees):/.test(app.selectedKey ?? '')}
        <Resizer width={panelW} bounds={PANEL} side="right" label={t('layout.resizePanel')} onchange={(w) => (panelW = w)} oncommit={(w) => saveSettings({ panelWidth: w })} />
        <Boundary name="panel" width="var(--panel-w, 340px)"><SidePanel /></Boundary>
      {/if}
    </div>
    {/if}
  </div>
</div>
{/if}
<Boundary name="overlay" onclose={() => app.open(null)}><Overlays /></Boundary>
{#if tour.phase}
  {#await tourView() then { default: Tour }}<Tour />{/await}
{/if}
{#if app.mobile && __SELF_UPDATE__}{#await apkHost() then { default: ApkUpdateHost }}<ApkUpdateHost />{/await}{/if}
<Toast />

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .views {
    display: contents;
  }
  .views.hidden {
    display: none;
  }
  main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
</style>
