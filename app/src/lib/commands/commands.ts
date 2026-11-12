import type { Component } from 'svelte';
import { isTauri } from '../daemon/client';
import { clamp } from '../clamp';
import { closedShells } from '../tabs/closed-tabs';
import { DEFAULT_SETTINGS, saveSettings, settings } from '../settings/settings.svelte';
import type { ShortcutId } from './shortcuts';
import { app } from '../state.svelte';
import { FONT_SIZE_MAX, FONT_SIZE_MIN } from '../terminal/terminal-font';
import { applyShortcut, endTarget, exitedTerm, tabInfo } from '../tabs/tabs';
import { openWindow } from '../window';
import { activeKey, activeTabs, closeTabAt, focusPane, openInFocused, paneInDirection, takeClosed, update, workspace, type Direction } from '../workspace/workspace.svelte';
import { createFolder, rescanFolders } from '../workspace/wsactions';
import { lineActions, resolveTab } from '../workspace/workspaces';

export type CommandIcon = Component<{ size?: number }>;

export interface Command {
  id: string;
  shortcut?: ShortcutId;
  available: () => boolean;
  run: (index?: number) => unknown;
  toggle?: () => unknown;
  label?: () => string;
  icon?: CommandIcon;
  section?: 'views' | 'actions';
  agentsView?: true;
}

export type CommandDef = Omit<Command, 'available'> & Partial<Pick<Command, 'available'>>;

export function selectTabKey(key: string) {
  if (key.startsWith('line:') || key.startsWith('term:')) app.select(key);
  else app.selectedKey = key;
}

let lastClose = 0;

export function closeTabKey(key: string, end: boolean, keepOtherSelection = false) {
  closeTabAt(workspace.layout.focused, key, end);
  if (keepOtherSelection && app.selectedKey !== key) return;
  const next = activeKey(workspace.layout);
  if (next) selectTabKey(next);
  else app.selectedKey = null;
}

export function answerCloseTab(end: boolean) {
  const key = app.overlay === 'closeTab' ? app.overlayArg : null;
  app.open(null);
  if (key) closeTabKey(key, end);
}

export function closeActive() {
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
    if (k.startsWith('term:') && app.connected && closedShells.has(k) && !app.data.terms.some((t: { id: string }) => t.id === k.slice(5))) return k;
    const r = resolveTab(k, app.items);
    if (!r || !tabInfo(r, app.items)) return null;
    if (r.startsWith('term:') && !app.data.terms.some((t: { id: string }) => t.id === r.slice(5))) return null;
    return r;
  });
  if (!key) return;
  const cwd = closedShells.get(key);
  if (cwd !== undefined && !app.data.terms.some((t: { id: string }) => t.id === key.slice(5))) {
    closedShells.delete(key);
    app.newSession('shell', { cwd }).catch((e: unknown) => app.notify(String((e as Error)?.message ?? e), 'error'));
    return;
  }
  update((l) => openInFocused(l, key));
  selectTabKey(key);
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
  if (key) selectTabKey(key);
}

function newTab() {
  if (settings.newTabShell && app.connected) {
    app.newSession('shell').catch((e) => app.notify(String((e as Error)?.message ?? e), 'error'));
    return;
  }
  window.dispatchEvent(new CustomEvent('sb:newtab'));
}

function termAction(action: 'find' | 'copy' | 'paste') {
  window.dispatchEvent(new CustomEvent('sb:term', { detail: action, cancelable: true }));
}

function fontSize(delta: number | null) {
  const size = delta === null ? DEFAULT_SETTINGS.fontSize : clamp(settings.fontSize + delta, FONT_SIZE_MIN, FONT_SIZE_MAX);
  if (size !== settings.fontSize) saveSettings({ fontSize: size });
}

export async function closeWindow() {
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

function split(dir: 'horizontal' | 'vertical') {
  window.dispatchEvent(new CustomEvent('sb:split', { detail: dir }));
}

function tabStep(step: { type: 'index'; index: number } | { type: 'next' } | { type: 'prev' }) {
  const l = workspace.layout;
  const key = applyShortcut(activeTabs(l), activeKey(l), step);
  if (key) selectTabKey(key);
}

const always = () => true;
export const connected = () => app.connected;
const persistent = () => app.connected && app.wsPersistent;

const core: CommandDef[] = [
  { id: 'kanban', shortcut: 'kanban', run: () => (app.view = 'kanban'), toggle: () => (app.view = app.view === 'kanban' ? 'agents' : 'kanban') },
  { id: 'panel', shortcut: 'togglePanel', run: () => (app.panelOpen = !app.panelOpen) },
  { id: 'source-control', shortcut: 'sourceControl', run: () => app.toggleSourceControl() },
  { id: 'split-right', shortcut: 'splitRight', run: () => split('horizontal') },
  { id: 'split-down', shortcut: 'splitDown', run: () => split('vertical') },
  { id: 'settings', shortcut: 'settings', run: () => app.open('settings') },
  { id: 'ws-new', shortcut: 'newWorkspace', available: persistent, run: () => app.open('workspaceNew') },
  { id: 'folder-new', available: persistent, run: () => createFolder() },
  { id: 'rescan-folders', available: connected, run: () => rescanFolders() },
  { id: 'shortcuts', shortcut: 'shortcuts', run: () => app.open('shortcuts'), toggle: () => app.open(app.overlay === 'shortcuts' ? null : 'shortcuts') },
  { id: 'new-window', shortcut: 'newWindow', run: () => newWindow() },
  { id: 'sidebar', shortcut: 'toggleSidebar', run: () => (app.sidebarOpen = !app.sidebarOpen) },
  { id: 'tab-new', shortcut: 'newTab', run: () => newTab() },
  { id: 'tab-close', shortcut: 'closeTab', run: () => closeActive() },
  { id: 'tab-reopen', shortcut: 'reopenTab', run: () => reopenClosed() },
  { id: 'palette', shortcut: 'palette', run: () => app.open('palette'), toggle: () => app.open(app.overlay === 'palette' ? null : 'palette') },
  { id: 'reconnect', shortcut: 'reconnect', run: () => (app.conn.phase !== 'connecting' ? void app.connect(app.conn.target) : undefined) },
  { id: 'tab-next', shortcut: 'nextTab', run: () => tabStep({ type: 'next' }) },
  { id: 'tab-prev', shortcut: 'prevTab', run: () => tabStep({ type: 'prev' }) },
  { id: 'tab-index', shortcut: 'tabIndex', run: (index = 0) => tabStep({ type: 'index', index }) },
  { id: 'focus-left', shortcut: 'focusLeft', run: () => focusDir('left') },
  { id: 'focus-right', shortcut: 'focusRight', run: () => focusDir('right') },
  { id: 'focus-up', shortcut: 'focusUp', run: () => focusDir('up') },
  { id: 'focus-down', shortcut: 'focusDown', run: () => focusDir('down') },
  { id: 'term-find', shortcut: 'find', run: () => termAction('find') },
  { id: 'term-copy', shortcut: 'copy', run: () => termAction('copy') },
  { id: 'term-paste', shortcut: 'paste', run: () => termAction('paste') },
  { id: 'font-up', shortcut: 'fontUp', run: () => fontSize(1) },
  { id: 'font-down', shortcut: 'fontDown', run: () => fontSize(-1) },
  { id: 'font-reset', shortcut: 'fontReset', run: () => fontSize(null) },
  { id: 'close-window', shortcut: 'closeWindow', run: () => closeWindow() },
];

const byId = new Map<string, Command>();
const byShortcut = new Map<ShortcutId, Command>();

export function register(list: CommandDef[]) {
  for (const d of list) {
    const c: Command = { available: always, ...d };
    byId.set(c.id, c);
    if (c.shortcut) byShortcut.set(c.shortcut, c);
  }
}

register(core);

export function allCommands(): Command[] {
  return [...byId.values()];
}

export function has(id: string): boolean {
  return byId.has(id);
}

export function command(id: string): Command {
  const c = byId.get(id);
  if (!c) throw new Error(`unknown command: ${id}`);
  return c;
}

export function runShortcut(s: { id: ShortcutId; index?: number }) {
  const c = byShortcut.get(s.id);
  if (!c || !c.available()) return;
  if (c.toggle) void c.toggle();
  else void c.run(s.index);
}
