import type { Component } from 'svelte';
import { isTauri } from './client';
import { closedShells } from './closed-tabs';
import { t, type Key } from './i18n.svelte';
import { DEFAULT_SETTINGS, saveSettings, settings } from './settings.svelte';
import type { ShortcutId } from './shortcuts';
import { app } from './state.svelte';
import { applyShortcut, endTarget, exitedTerm, tabInfo } from './tabs';
import { openWindow } from './window';
import { activeKey, activeTabs, closeTabAt, focusPane, openInFocused, paneInDirection, takeClosed, update, workspace, type Direction } from './workspace.svelte';
import { createFolder, rescanFolders } from './wsactions';
import { lineActions, resolveTab } from './workspaces';

export type CommandIcon = Component<{ size?: number }>;

export interface Command {
  id: string;
  label: () => string;
  icon?: CommandIcon;
  section: 'views' | 'actions';
  shortcut?: ShortcutId;
  agentsView?: true;
  available: () => boolean;
  run: (index?: number) => unknown;
  toggle?: () => unknown;
}

export type CommandDef = Omit<Command, 'section' | 'available'> & Partial<Pick<Command, 'section' | 'available'>>;

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

function termAction(action: 'find' | 'copy' | 'paste') {
  window.dispatchEvent(new CustomEvent('sb:term', { detail: action, cancelable: true }));
}

function fontSize(delta: number | null) {
  const size = delta === null ? DEFAULT_SETTINGS.fontSize : Math.min(24, Math.max(9, settings.fontSize + delta));
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
  if (key) activate(key);
}

const always = () => true;
export const connected = () => app.connected;
const persistent = () => app.connected && app.wsPersistent;

export const label = (key: Key) => () => t(key);
const keyed = (id: ShortcutId): (() => string) => () => t(`keys.${id}` as Key);

const core: CommandDef[] = [
  { id: 'kanban', label: label('palette.openKanban'), shortcut: 'kanban', run: () => (app.view = 'kanban'), toggle: () => (app.view = app.view === 'kanban' ? 'agents' : 'kanban') },
  { id: 'panel', label: label('palette.togglePanel'), shortcut: 'togglePanel', run: () => (app.panelOpen = !app.panelOpen) },
  { id: 'source-control', label: label('palette.sourceControl'), shortcut: 'sourceControl', run: () => app.toggleSourceControl() },
  { id: 'split-right', label: label('palette.splitRight'), shortcut: 'splitRight', agentsView: true, run: () => split('horizontal') },
  { id: 'split-down', label: label('palette.splitDown'), shortcut: 'splitDown', agentsView: true, run: () => split('vertical') },
  { id: 'settings', label: label('palette.settings'), shortcut: 'settings', run: () => app.open('settings') },
  { id: 'ws-new', label: label('palette.newWorkspace'), shortcut: 'newWorkspace', available: persistent, run: () => app.open('workspaceNew') },
  { id: 'folder-new', label: label('palette.newFolder'), available: persistent, run: () => createFolder() },
  { id: 'rescan-folders', label: label('palette.rescanFolders'), available: connected, run: () => rescanFolders() },
  { id: 'shortcuts', label: label('palette.shortcuts'), shortcut: 'shortcuts', run: () => app.open('shortcuts'), toggle: () => app.open(app.overlay === 'shortcuts' ? null : 'shortcuts') },
  { id: 'new-window', label: label('palette.newWindow'), shortcut: 'newWindow', run: () => newWindow() },
  { id: 'sidebar', label: label('palette.toggleSidebar'), shortcut: 'toggleSidebar', run: () => (app.sidebarOpen = !app.sidebarOpen) },
  { id: 'tab-new', label: label('palette.newTab'), shortcut: 'newTab', agentsView: true, run: () => newTab() },
  { id: 'tab-close', label: label('palette.closeTab'), shortcut: 'closeTab', agentsView: true, run: () => closeActive() },
  { id: 'tab-reopen', label: label('palette.reopenTab'), shortcut: 'reopenTab', agentsView: true, run: () => reopenClosed() },
  { id: 'palette', label: label('title.palette'), shortcut: 'palette', run: () => app.open('palette'), toggle: () => app.open(app.overlay === 'palette' ? null : 'palette') },
  { id: 'reconnect', label: keyed('reconnect'), shortcut: 'reconnect', run: () => (app.conn.phase !== 'connecting' ? void app.connect(app.conn.target) : undefined) },
  { id: 'tab-next', label: keyed('nextTab'), shortcut: 'nextTab', run: () => tabStep({ type: 'next' }) },
  { id: 'tab-prev', label: keyed('prevTab'), shortcut: 'prevTab', run: () => tabStep({ type: 'prev' }) },
  { id: 'tab-index', label: keyed('tabIndex'), shortcut: 'tabIndex', run: (index = 0) => tabStep({ type: 'index', index }) },
  { id: 'focus-left', label: keyed('focusLeft'), shortcut: 'focusLeft', run: () => focusDir('left') },
  { id: 'focus-right', label: keyed('focusRight'), shortcut: 'focusRight', run: () => focusDir('right') },
  { id: 'focus-up', label: keyed('focusUp'), shortcut: 'focusUp', run: () => focusDir('up') },
  { id: 'focus-down', label: keyed('focusDown'), shortcut: 'focusDown', run: () => focusDir('down') },
  { id: 'term-find', label: keyed('find'), shortcut: 'find', run: () => termAction('find') },
  { id: 'term-copy', label: keyed('copy'), shortcut: 'copy', run: () => termAction('copy') },
  { id: 'term-paste', label: keyed('paste'), shortcut: 'paste', run: () => termAction('paste') },
  { id: 'font-up', label: keyed('fontUp'), shortcut: 'fontUp', run: () => fontSize(1) },
  { id: 'font-down', label: keyed('fontDown'), shortcut: 'fontDown', run: () => fontSize(-1) },
  { id: 'font-reset', label: keyed('fontReset'), shortcut: 'fontReset', run: () => fontSize(null) },
  { id: 'close-window', label: keyed('closeWindow'), shortcut: 'closeWindow', run: () => closeWindow() },
];

const byId = new Map<string, Command>();
const byShortcut = new Map<ShortcutId, Command>();

export function register(list: CommandDef[]) {
  for (const d of list) {
    const c: Command = { section: 'actions', available: always, ...d };
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
