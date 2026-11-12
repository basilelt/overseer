import { isTauri } from '../daemon/client';
import { answerCloseTab, closeActive, closeTabKey, command, newWindow, runShortcut } from './commands';
import { ui } from '../mobile/ui.svelte';
import { phoneOrMobile } from '../mobile/phone.svelte';
import { isMac } from '../platform';
import { settings } from '../settings/settings.svelte';
import { accelerator, resolveShortcut, shortcutLabel, type ShortcutId } from './shortcuts';
import { app } from '../state.svelte';
import { tour } from '../tour/tour.svelte';
import { windowLabel } from '../window';

export { answerCloseTab, closeTabKey, newWindow, runShortcut };

export { isMac };

const GLOBAL = new Set<ShortcutId>(['palette', 'settings', 'shortcuts', 'kanban', 'toggleSidebar', 'sourceControl', 'togglePanel', 'newWorkspace', 'reconnect', 'fontUp', 'fontDown', 'fontReset', 'newWindow', 'closeWindow']);
const WITH_OVERLAY = new Set<ShortcutId>(['palette', 'settings', 'shortcuts']);

export function hint(id: ShortcutId): string {
  return shortcutLabel(id, { mac: isMac, ctrlWClosesTab: settings.ctrlWClosesTab });
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
  if (e.isComposing || phoneOrMobile() || tour.phase) return;
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
        { id: id('new-window'), text: 'New Window', accelerator: accelerator('newWindow'), action: () => void command('new-window').run() },
        { id: id('close-tab'), text: 'Close Tab', accelerator: accelerator('closeTab'), action: close },
        { id: id('close-window'), text: 'Close Window', accelerator: accelerator('closeWindow'), action: () => void command('close-window').run() },
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
