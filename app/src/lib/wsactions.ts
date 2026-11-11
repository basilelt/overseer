import Archive from '@lucide/svelte/icons/archive';
import ArchiveRestore from '@lucide/svelte/icons/archive-restore';
import Bell from '@lucide/svelte/icons/bell';
import BellOff from '@lucide/svelte/icons/bell-off';
import Gauge from '@lucide/svelte/icons/gauge';
import FolderInput from '@lucide/svelte/icons/folder-input';
import FolderPlus from '@lucide/svelte/icons/folder-plus';
import FolderTree from '@lucide/svelte/icons/folder-tree';
import Home from '@lucide/svelte/icons/house';
import Megaphone from '@lucide/svelte/icons/megaphone';
import Pause from '@lucide/svelte/icons/pause';
import Pencil from '@lucide/svelte/icons/pencil';
import Pin from '@lucide/svelte/icons/pin';
import PinOff from '@lucide/svelte/icons/pin-off';
import Play from '@lucide/svelte/icons/play';
import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
import SquareTerminal from '@lucide/svelte/icons/square-terminal';
import SquarePlus from '@lucide/svelte/icons/square-plus';
import Undo2 from '@lucide/svelte/icons/undo-2';
import Trash from '@lucide/svelte/icons/trash-2';
import Users from '@lucide/svelte/icons/users';
import X from '@lucide/svelte/icons/x';
import { t, type Key } from './i18n.svelte';
import type { MenuItem } from './menu';
import { app, NO_WORKSPACE, type BroadcastTarget } from './state.svelte';
import type { Folder, Group, Workspace } from './types';
import { byRank, lineActions, type LineAction } from './workspaces';
import { runSessionAction } from './session-action';
import { movableStatuses, taskForItem } from './kanban-core';
import { isTargetMuted, toggleTargetMute } from './os-notify';
import { canInstall, ordered } from './agents';
import { saveSettings, settings } from './settings.svelte';
import { tabLazy } from './state.svelte';

const muteItem = (target: string): MenuItem => ({ label: t('nb.mute'), icon: BellOff, checked: isTargetMuted(app, target), run: () => toggleTargetMute(app, target) });

const fail = (e: unknown) => app.notify(String((e as Error)?.message ?? e), 'error');

export function askName(title: string, label: string, value: string, submit: (name: string) => Promise<unknown> | unknown) {
  app.nameDialog = { title, label, value, submit };
  app.open('name');
}

export function broadcastTo(target: BroadcastTarget) {
  app.broadcastTarget = target;
  app.open('broadcast');
}

export function holdAll(target: BroadcastTarget, mode: 'hold' | 'resume' | 'close') {
  app.broadcastTarget = target;
  app.open('holdAll', mode);
}

export function targetItems(target: BroadcastTarget) {
  if (target.kind === 'workspace') return app.membersOf(target.id);
  if (target.kind === 'group') return app.groupItems(target.id);
  return app.items.filter((i) => target.keys.includes(i.key));
}

export function targetName(target: BroadcastTarget): string {
  if (target.kind === 'workspace') return app.ws.workspaces.find((w) => w.id === target.id)?.name ?? t('ws.other');
  if (target.kind === 'group') return app.ws.groups.find((g) => g.id === target.id)?.name ?? '';
  return t('ws.selection', { n: target.keys.length });
}

export async function rescanFolders() {
  if (!app.connected) return;
  app.progress(t('ws.rescanning'));
  try {
    const { added } = await app.api.discoverProjects();
    await app.refresh();
    app.notify(added ? t(added === 1 ? 'ws.foundOne' : 'ws.foundMany', { n: added }) : t('ws.foundNone'));
  } catch (e) {
    fail(e);
  }
}

export async function createFolder(parentId: string | null = null) {
  askName(t('ws.newFolder'), t('ws.name'), '', async (name) => {
    const f = await app.api.createFolder({ name, parentId });
    app.ws = { ...app.ws, folders: [...app.ws.folders.filter((x) => x.id !== f.id), f] };
  });
}

export async function createGroup(keys: string[]) {
  askName(t('grp.new'), t('ws.name'), '', async (name) => {
    const g = await app.api.createGroup({ name, members: keys });
    app.ws = { ...app.ws, groups: [...app.ws.groups.filter((x) => x.id !== g.id), g] };
    app.multi = new Set();
  });
}

export async function addToGroup(g: Group, keys: string[]) {
  try {
    const next = await app.api.patchGroup(g.id, { add: keys });
    app.ws = { ...app.ws, groups: app.ws.groups.map((x) => (x.id === next.id ? next : x)) };
    app.multi = new Set();
  } catch (e) {
    fail(e);
  }
}

export function groupPicker(keys: string[]): MenuItem[] {
  return [
    ...app.ws.groups
      .slice()
      .sort(byRank)
      .map((g) => ({ label: g.name, icon: Users, run: () => addToGroup(g, keys) })),
    ...(app.ws.groups.length ? [{ label: '', separator: true }] : []),
    { label: t('grp.new'), icon: SquarePlus, run: () => createGroup(keys) },
  ];
}

// The menu of the tab strip "+", also opened from a nested workspace's "+": agents, then the shell.
export function newSessionItems(): MenuItem[] {
  const start = (id: string) => () => app.newSession(id).catch(fail);
  const install = (id: string) => () => app.installAgent(id).catch(fail);
  const agents = ordered(app.agents).filter((a) => a.id !== 'shell');
  const shell = app.agents.find((a) => a.id === 'shell');
  return [
    ...agents.map((a) => ({ label: canInstall(a) ? t('palette.installAgent', { name: a.name }) : a.name, agent: a.id, default: a.id === settings.defaultAgent, disabled: !a.installed && !canInstall(a), hint: a.installed ? undefined : (a.install ?? t('agents.notInstalled')), run: a.installed ? start(a.id) : install(a.id) })),
    { label: t('tabs.inCheckout'), checked: settings.sessionsInCheckout, run: () => saveSettings({ sessionsInCheckout: !settings.sessionsInCheckout }) },
    { label: '', separator: true },
    { label: shell?.name ?? t('tabs.shell'), agent: 'shell', run: start('shell') },
  ];
}

// The "+" of a nested workspace: selects it, then offers the new-session menu with where sessions start.
export function startIn(e: { clientX: number; clientY: number }, w: Workspace) {
  app.selectWorkspace(w.id);
  const dir = w.cwd ?? w.path;
  app.openMenu(e, t('ws.sub.startHere', { name: w.name }), [...newSessionItems(), ...(dir ? [{ label: '', separator: true }, { label: t('ws.sub.startsIn', { path: dir }), disabled: true }] : [])]);
}

export function workspaceMenu(w: Workspace): MenuItem[] {
  const persistent = app.wsPersistent;
  const folders = app.ws.folders.slice().sort(byRank);
  const parent = w.parentId ? app.ws.workspaces.find((p) => p.id === w.parentId) : undefined;
  const top = !w.parentId;
  return [
    { label: t('ws.rename'), icon: Pencil, disabled: !persistent, run: () => askName(t('ws.rename'), t('ws.name'), w.name, (name) => app.patchWorkspace(w.id, { name })) },
    ...(top ? [{ label: w.pinned ? t('ws.unpin') : t('ws.pin'), icon: w.pinned ? PinOff : Pin, disabled: !persistent, run: () => app.patchWorkspace(w.id, { pinned: !w.pinned }) }] : []),
    ...(top && w.path && w.kind !== 'subject' ? [{ label: t('ws.sub.new'), icon: FolderTree, disabled: !persistent, run: () => app.open('workspaceNew', w.id) }] : []),
    ...(parent?.path && w.kind !== 'subject'
      ? [{ label: t('ws.sub.startAtRoot'), icon: Home, checked: w.cwd === parent.path, disabled: !persistent, run: () => app.patchWorkspace(w.id, { cwd: w.cwd === parent.path ? null : parent.path }) }]
      : []),
    ...(top
      ? folders
          .filter((f) => f.id !== w.folderId)
          .map((f) => ({ label: t('ws.moveTo', { name: f.parentId ? `${folders.find((p) => p.id === f.parentId)?.name ?? ''} / ${f.name}` : f.name }), icon: FolderInput, disabled: !persistent, run: () => app.patchWorkspace(w.id, { folderId: f.id, pinned: false }) }))
      : []),
    ...(top && w.folderId ? [{ label: t('ws.moveTop'), icon: FolderInput, disabled: !persistent, run: () => app.patchWorkspace(w.id, { folderId: null }) }] : []),
    { label: '', separator: true },
    { label: t('broadcast.title'), icon: Megaphone, run: () => broadcastTo({ kind: 'workspace', id: w.id }) },
    { label: t('hold.all'), icon: Pause, run: () => holdAll({ kind: 'workspace', id: w.id }, 'hold') },
    { label: t('hold.resumeAll'), icon: Play, run: () => holdAll({ kind: 'workspace', id: w.id }, 'resume') },
    { label: t('ws.notifyIdle'), icon: Bell, checked: w.notifyIdle, disabled: !persistent, run: () => app.patchWorkspace(w.id, { notifyIdle: !w.notifyIdle }) },
    muteItem(`w:${w.id}`),
    ...(app.profiles && w.path ? [{ label: t('profile.forWorkspace'), icon: Gauge, hint: app.profiles.profiles.find((p) => p.id === app.profiles?.scopes[w.path!])?.name, run: () => app.open('profiles', w.path!) }] : []),
    { label: '', separator: true },
    { label: w.archived ? t('ws.unarchive') : t('ws.archive'), icon: w.archived ? ArchiveRestore : Archive, disabled: !persistent, run: () => app.patchWorkspace(w.id, { archived: !w.archived }) },
    {
      label: t('common.delete'),
      icon: Trash,
      danger: true,
      disabled: !persistent,
      run: () =>
        app.api
          .deleteWorkspace(w.id)
          .then(() => (app.ws = { ...app.ws, workspaces: app.ws.workspaces.filter((x) => x.id !== w.id) }))
          .catch(fail),
    },
  ];
}

export function folderMenu(f: Folder): MenuItem[] {
  return [
    {
      label: t('ws.rename'),
      icon: Pencil,
      run: () =>
        askName(t('ws.rename'), t('ws.name'), f.name, async (name) => {
          const next = await app.api.patchFolder(f.id, { name });
          app.ws = { ...app.ws, folders: app.ws.folders.map((x) => (x.id === next.id ? next : x)) };
        }),
    },
    ...(f.parentId ? [] : [{ label: t('ws.newSubfolder'), icon: FolderPlus, run: () => createFolder(f.id) }]),
    { label: '', separator: true },
    {
      label: t('common.delete'),
      icon: Trash,
      danger: true,
      run: () =>
        app.api
          .deleteFolder(f.id)
          .then(() => app.refresh())
          .catch(fail),
    },
  ];
}

export function groupMenu(g: Group): MenuItem[] {
  return [
    { label: t('broadcast.title'), icon: Megaphone, run: () => broadcastTo({ kind: 'group', id: g.id }) },
    { label: t('hold.all'), icon: Pause, run: () => holdAll({ kind: 'group', id: g.id }, 'hold') },
    { label: t('hold.resumeAll'), icon: Play, run: () => holdAll({ kind: 'group', id: g.id }, 'resume') },
    {
      label: t('ws.notifyIdle'),
      icon: Bell,
      checked: g.notifyIdle,
      run: () =>
        app.api
          .patchGroup(g.id, { notifyIdle: !g.notifyIdle })
          .then((next) => (app.ws = { ...app.ws, groups: app.ws.groups.map((x) => (x.id === next.id ? next : x)) }))
          .catch(fail),
    },
    muteItem(`g:${g.id}`),
    { label: '', separator: true },
    {
      label: t('ws.rename'),
      icon: Pencil,
      run: () =>
        askName(t('ws.rename'), t('ws.name'), g.name, async (name) => {
          const next = await app.api.patchGroup(g.id, { name });
          app.ws = { ...app.ws, groups: app.ws.groups.map((x) => (x.id === next.id ? next : x)) };
        }),
    },
    {
      label: t('common.delete'),
      icon: Trash,
      danger: true,
      run: () =>
        app.api
          .deleteGroup(g.id)
          .then(() => (app.ws = { ...app.ws, groups: app.ws.groups.filter((x) => x.id !== g.id) }))
          .catch(fail),
    },
  ];
}

export function otherMenu(): MenuItem[] {
  const target = { kind: 'workspace' as const, id: NO_WORKSPACE };
  return [
    { label: t('broadcast.title'), icon: Megaphone, run: () => broadcastTo(target) },
    { label: t('hold.all'), icon: Pause, run: () => holdAll(target, 'hold') },
    { label: t('hold.resumeAll'), icon: Play, run: () => holdAll(target, 'resume') },
    { label: '', separator: true },
    { label: t('hold.closeAll'), icon: X, danger: true, run: () => holdAll(target, 'close') },
  ];
}

const LINE_ACTION: Record<LineAction, { label: Key; icon: MenuItem['icon'] }> = {
  open: { label: 'line.open', icon: SquareTerminal },
  attach: { label: 'line.attach', icon: SquareTerminal },
  resume: { label: 'action.resume', icon: RotateCcw },
  hold: { label: 'action.hold', icon: Pause },
  close: { label: 'line.close', icon: X },
};

function runLineAction(a: LineAction, key: string, keys: string[]) {
  const item = app.items.find((i) => i.key === key);
  if (!item?.lineId) return;
  if (a === 'close') return holdAll({ kind: 'items', keys }, 'close');
  if (a === 'hold') return holdAll({ kind: 'items', keys }, 'hold');
  return (a === 'resume' ? app.resumeLine(item) : app.openTerm(item))
    .then(() => app.select(key))
    .catch(fail);
}

function pauseLine(lineId: string, kind: 'pause' | 'cancelPause') {
  return runSessionAction(app.api, lineId, kind)
    .then((r) => r.notice && app.notify(t(r.notice)))
    .catch((e) => app.notify(t('chat.actionFailed', { reason: String((e as Error)?.message ?? e) }), 'error'));
}

function lineMenu(key: string, keys: string[]): MenuItem[] {
  const item = app.items.find((i) => i.key === key);
  const line = item?.lineId ? app.data.lines.find((l) => l.line.id === item.lineId) : undefined;
  const acts = item ? lineActions(item, line) : [];
  if (!acts.length) return [];
  const single = keys.length === 1 && !!item?.lineId;
  const pending = !!line?.local?.holdPending;
  return [
    ...acts.map((a) =>
      a === 'hold' && single
        ? {
            label: t(pending ? 'session.action.cancelPause' : 'session.action.pause'),
            icon: pending ? Undo2 : Pause,
            run: () => pauseLine(item!.lineId!, pending ? 'cancelPause' : 'pause'),
          }
        : { label: t(LINE_ACTION[a].label), icon: LINE_ACTION[a].icon, danger: a === 'close', run: () => runLineAction(a, key, keys) },
    ),
    { label: '', separator: true },
  ];
}

function renameMenu(key: string, keys: string[]): MenuItem[] {
  const item = keys.length === 1 ? app.items.find((i) => i.key === key) : undefined;
  if (!item || (!item.lineId && !item.termId)) return [];
  return [
    { label: t('tabs.rename'), icon: Pencil, run: () => askName(t('tabs.renameTitle'), t('tabs.renameLabel'), item.title, (name) => app.renameItem(key, name)) },
    ...(item.renamed ? [{ label: t('tabs.resetTitle'), icon: RotateCcw, run: () => void app.renameItem(key, null) }] : []),
    { label: '', separator: true },
  ];
}

export function itemMenu(key: string): MenuItem[] {
  const keys = app.multi.size && app.multi.has(key) ? [...app.multi] : [key];
  const inGroups = app.ws.groups.filter((g) => keys.every((k) => g.members.includes(k)));
  const task = keys.length === 1 ? taskForItem(app.data.tasks, app.items.find((i) => i.key === key)) : undefined;
  const status: MenuItem[] = task
    ? [
        ...movableStatuses(task).map((s) => ({
          label: t('tasks.moveTo', { status: t(`kanban.col.${s}` as Key) }),
          checked: task.status === s,
          disabled: task.status === s,
          run: () => void app.api.patchTask(task.id, { status: s }).catch(fail),
        })),
        { label: '', separator: true },
      ]
    : [];
  return [
    ...lineMenu(key, keys),
    ...renameMenu(key, keys),
    ...status,
    ...(keys.length === 1 && app.canMoveToWorkspace(key)
      ? [{ label: t('ws.moveSession'), icon: FolderInput, run: () => void tabLazy().then((m) => app.openMenu(lastMenuAt, t('ws.moveSession'), m.moveMenu(app, key))) }]
      : []),
    { label: t('broadcast.title'), icon: Megaphone, run: () => broadcastTo({ kind: 'items', keys }) },
    ...(keys.length === 1 ? [muteItem(key)] : []),
    { label: '', separator: true },
    ...groupPicker(keys).map((m) => (m.separator ? m : { ...m, label: m.icon === Users ? t('grp.addTo', { name: m.label }) : m.label })),
    ...inGroups.map((g) => ({
      label: t('grp.removeFrom', { name: g.name }),
      icon: Users,
      run: () =>
        app.api
          .patchGroup(g.id, { remove: keys })
          .then((next) => (app.ws = { ...app.ws, groups: app.ws.groups.map((x) => (x.id === next.id ? next : x)) }))
          .catch(fail),
    })),
  ];
}

// Click on a session row: Ctrl/Meta/Shift toggles it in the multi-selection, a plain click opens it alone.
export function selectSession(key: string, e?: MouseEvent) {
  if (e && (e.metaKey || e.ctrlKey || e.shiftKey)) {
    const next = new Set(app.multi);
    if (!next.delete(key)) next.add(key);
    if (app.selectedKey && !next.size) next.add(app.selectedKey);
    app.multi = next;
    return;
  }
  app.multi = new Set();
  app.select(key);
}

let lastMenuAt = { clientX: 0, clientY: 0 };

export const itemMenuAt = (key: string, title: string, e: MouseEvent) => {
  lastMenuAt = { clientX: e.clientX, clientY: e.clientY };
  app.openMenu(e, title, itemMenu(key));
};
