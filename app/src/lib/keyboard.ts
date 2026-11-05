import { isTauri } from './client';
import { ui } from './mobile/ui.svelte';
import { DEFAULT_SETTINGS, saveSettings, settings } from './settings.svelte';
import { resolveShortcut, shortcutLabel, type Resolved, type ShortcutId } from './shortcuts';
import { app } from './state.svelte';
import { tour } from './tour/tour.svelte';
import { applyShortcut, endTarget, exitedTerm, tabInfo } from './tabs';
import { activeKey, activeTabs, closeTabAt, focusPane, openInFocused, paneInDirection, takeClosed, update, workspace, type Direction } from './workspace.svelte';
import { lineActions, resolveTab } from './workspaces';
import { openWindow, windowLabel } from './window';

export const isMac = typeof navigator !== 'undefined' && /mac/i.test(navigator.platform);

export type TermAction = 'find' | 'copy' | 'paste';

const GLOBAL = new Set<ShortcutId>(['palette', 'settings', 'shortcuts', 'kanban', 'toggleSidebar', 'togglePanel', 'newWorkspace', 'reconnect', 'fontUp', 'fontDown', 'fontReset', 'newWindow', 'closeWindow']);
const WITH_OVERLAY = new Set<ShortcutId>(['palette', 'settings', 'shortcuts']);

export function hint(id: ShortcutId): string {
  return shortcutLabel(id, { mac: isMac, ctrlWClosesTab: settings.ctrlWClosesTab });
}

function activate(key: string) {
  if (key.startsWith('line:') || key.startsWith('term:')) app.select(key);
  else app.selectedKey = key;
}

let lastClose = 0;

export function closeTabKey(key: string, end: boolean) {
  closeTabAt(workspace.layout.focused, key, end);
  const next = activeKey(workspace.layout);
  if (next) activate(next);
  else app.selectedKey = null;
}

export function answerCloseTab(end: boolean) {
  const key = app.overlay === 'closeTab' ? app.overlayArg : null;
  app.open(null);
  if (key) closeTabKey(key, end);
}

function closeActive() {
  const now = Date.now();
  if (now - lastClose < 150) return;
  lastClose = now;
  const key = activeKey(workspace.layout);
  if (!key) return;
  if (endTarget(key, app.items) && !exitedTerm(key, app.data.terms)) app.open('closeTab', key);
  else closeTabKey(key, true);
}

function reopenClosed() {
  const key = takeClosed((k) => {
    const r = resolveTab(k, app.items);
    return r && tabInfo(r, app.items) ? r : null;
  });
  if (!key) return;
  update((l) => openInFocused(l, key));
  activate(key);
  const item = app.items.find((i) => i.key === key);
  if (!item?.lineId || item.termId) return;
  if (!lineActions(item, app.data.lines.find((l) => l.line.id === item.lineId)).includes('attach')) return;
  app.openTerm(item).catch((e) => app.notify(String((e as Error)?.message ?? e), 'error'));
}

function focusDir(dir: Direction) {
  const to = paneInDirection(workspace.layout, dir);
  if (to === null) return;
  update((l) => focusPane(l, to));
  const key = activeKey(workspace.layout);
  if (key) activate(key);
}

function newTab() {
  if (settings.newTabShell && app.connected) {
    app.newSession('shell').catch((e) => app.notify(String((e as Error)?.message ?? e), 'error'));
    return;
  }
  window.dispatchEvent(new CustomEvent('sb:newtab'));
}

function termAction(action: TermAction): boolean {
  const ev = new CustomEvent('sb:term', { detail: action, cancelable: true });
  window.dispatchEvent(ev);
  return ev.defaultPrevented;
}

function fontSize(delta: number | null) {
  const size = delta === null ? DEFAULT_SETTINGS.fontSize : Math.min(24, Math.max(9, settings.fontSize + delta));
  if (size !== settings.fontSize) saveSettings({ fontSize: size });
}

async function closeWindow() {
  if (!isTauri()) return;
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().close();
  } catch {}
}

export async function newWindow() {
  try {
    await openWindow();
  } catch (e) {
    app.notify(String((e as Error)?.message ?? e), 'error');
  }
}

export function runShortcut(s: Resolved) {
  switch (s.id) {
    case 'palette':
      return app.open(app.overlay === 'palette' ? null : 'palette');
    case 'settings':
      return app.open('settings');
    case 'shortcuts':
      return app.open(app.overlay === 'shortcuts' ? null : 'shortcuts');
    case 'kanban':
      app.view = app.view === 'kanban' ? 'agents' : 'kanban';
      return;
    case 'toggleSidebar':
      app.sidebarOpen = !app.sidebarOpen;
      return;
    case 'togglePanel':
      app.panelOpen = !app.panelOpen;
      return;
    case 'reconnect':
      if (app.conn.phase !== 'connecting') void app.connect(app.conn.target);
      return;
    case 'newWorkspace':
      if (app.connected && app.wsPersistent) app.open('workspaceNew');
      return;
    case 'fontUp':
      return fontSize(1);
    case 'fontDown':
      return fontSize(-1);
    case 'fontReset':
      return fontSize(null);
    case 'closeWindow':
      return void closeWindow();
    case 'newWindow':
      return void newWindow();
    case 'newTab':
      return newTab();
    case 'closeTab':
      return closeActive();
    case 'reopenTab':
      return reopenClosed();
    case 'nextTab':
    case 'prevTab':
    case 'tabIndex': {
      const l = workspace.layout;
      const key = applyShortcut(activeTabs(l), activeKey(l), s.id === 'tabIndex' ? { type: 'index', index: s.index ?? 0 } : { type: s.id === 'nextTab' ? 'next' : 'prev' });
      if (key) activate(key);
      return;
    }
    case 'splitRight':
    case 'splitDown':
      window.dispatchEvent(new CustomEvent('sb:split', { detail: s.id === 'splitRight' ? 'horizontal' : 'vertical' }));
      return;
    case 'focusLeft':
      return focusDir('left');
    case 'focusRight':
      return focusDir('right');
    case 'focusUp':
      return focusDir('up');
    case 'focusDown':
      return focusDir('down');
    case 'find':
      return void termAction('find');
    case 'copy':
      return void termAction('copy');
    case 'paste':
      return void termAction('paste');
  }
}

export function allowed(id: ShortcutId): boolean {
  if (app.overlay) return WITH_OVERLAY.has(id);
  if (GLOBAL.has(id)) return true;
  return app.view === 'agents' && !app.menu;
}

function inTerminal(e: Event): boolean {
  const t = e.target;
  return t instanceof Element && !!t.closest('[data-terminal]');
}

function onKey(e: KeyboardEvent) {
  if (e.isComposing || app.mobile || ui.layout === 'phone' || tour.phase) return;
  const s = resolveShortcut(e, { mac: isMac, ctrlWClosesTab: settings.ctrlWClosesTab, inTerminal: inTerminal(e) });
  if (!s) return;
  if (!allowed(s.id)) {
    if (s.id === 'closeTab') e.preventDefault();
    return;
  }
  e.preventDefault();
  e.stopPropagation();
  runShortcut(s);
}

async function macMenu(): Promise<() => void> {
  try {
    const { Menu, Submenu } = await import('@tauri-apps/api/menu');
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const id = (name: string) => `sb-${name}-${windowLabel()}`;
    const close = () => {
      if (app.overlay) app.open(null);
      else if (allowed('closeTab')) closeActive();
    };
    const appMenu = await Submenu.new({
      text: 'Overseer',
      items: [{ item: { About: null } }, { item: 'Separator' }, { item: 'Services' }, { item: 'Separator' }, { item: 'Hide' }, { item: 'HideOthers' }, { item: 'ShowAll' }, { item: 'Separator' }, { item: 'Quit' }],
    });
    const file = await Submenu.new({
      text: 'File',
      items: [
        { id: id('new-window'), text: 'New Window', accelerator: 'CmdOrCtrl+Alt+N', action: () => void newWindow() },
        { id: id('close-tab'), text: 'Close Tab', accelerator: 'CmdOrCtrl+W', action: close },
        { id: id('close-window'), text: 'Close Window', accelerator: 'CmdOrCtrl+Shift+W', action: () => void closeWindow() },
      ],
    });
    const edit = await Submenu.new({
      text: 'Edit',
      items: [{ item: 'Undo' }, { item: 'Redo' }, { item: 'Separator' }, { item: 'Cut' }, { item: 'Copy' }, { item: 'Paste' }, { item: 'SelectAll' }],
    });
    const view = await Submenu.new({ text: 'View', items: [{ item: 'Fullscreen' }] });
    const win = await Submenu.new({ text: 'Window', items: [{ item: 'Minimize' }, { item: 'Maximize' }, { item: 'Separator' }, { item: 'BringAllToFront' }] });
    const menu = await Menu.new({ items: [appMenu, file, edit, view, win] });
    const own = async () => {
      await menu.setAsAppMenu();
      await win.setAsWindowsMenuForNSApp().catch(() => {});
    };
    await own();
    return await getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) void own().catch(() => {});
    });
  } catch {
    return () => {};
  }
}

export function startShortcuts(): () => void {
  window.addEventListener('keydown', onKey, true);
  let stopMenu: (() => void) | null = null;
  let stopped = false;
  if (isMac && isTauri() && !ui.mobileOs)
    void macMenu().then((u) => {
      if (stopped) u();
      else stopMenu = u;
    });
  return () => {
    stopped = true;
    stopMenu?.();
    window.removeEventListener('keydown', onKey, true);
  };
}
