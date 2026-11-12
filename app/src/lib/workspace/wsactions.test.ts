import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { app, moveMenu } = vi.hoisted(() => ({ app: {} as Record<string, any>, moveMenu: vi.fn(() => [{ label: 'webshop' }]) }));

vi.mock('../state.svelte', () => ({ app, NO_WORKSPACE: '', tabLazy: async () => ({ moveMenu }) }));

import { t } from '../i18n.svelte';
import type { MenuItem } from '../commands/menu';
import { settings } from '../settings/settings.svelte';
import type { Folder, Group, Workspace } from '../types';
import {
  addToGroup,
  askName,
  broadcastTo,
  createFolder,
  createGroup,
  folderMenu,
  groupMenu,
  groupPicker,
  holdAll,
  itemMenu,
  otherMenu,
  rescanFolders,
  newSessionItems,
  selectSession,
  startIn,
  itemMenuAt,
  targetItems,
  targetName,
  workspaceMenu,
} from './wsactions';

const STAMP = { createdAt: '', updatedAt: '' };

const workspace = (over: Partial<Workspace> = {}): Workspace => ({
  id: 'w1',
  name: 'Alpha',
  path: '/src/alpha',
  folderId: null,
  pinned: false,
  archived: false,
  auto: false,
  rank: 0,
  tabs: [],
  notifyIdle: false,
  ...STAMP,
  ...over,
});

const folder = (over: Partial<Folder> = {}): Folder => ({ id: 'f1', name: 'Work', parentId: null, rank: 0, collapsed: false, ...STAMP, ...over });

const group = (over: Partial<Group> = {}): Group => ({ id: 'g1', name: 'Team', rank: 0, members: [], notifyIdle: false, ...STAMP, ...over });

function api() {
  return {
    discoverProjects: vi.fn(async () => ({ added: 0 })),
    createFolder: vi.fn(async (b: { name: string; parentId: string | null }) => folder({ id: 'fnew', ...b })),
    createGroup: vi.fn(async (b: { name: string; members: string[] }) => group({ id: 'gnew', ...b })),
    patchGroup: vi.fn(async (id: string, p: Record<string, unknown>) => ({ ...app.ws.groups.find((g: Group) => g.id === id), ...p, patched: true })),
    patchFolder: vi.fn(async (id: string, p: Record<string, unknown>) => ({ ...app.ws.folders.find((f: Folder) => f.id === id), ...p })),
    deleteWorkspace: vi.fn(async () => {}),
    deleteFolder: vi.fn(async () => {}),
    deleteGroup: vi.fn(async () => {}),
    patchTask: vi.fn(async () => {}),
  };
}

function freshApp() {
  return {
    connected: true,
    wsPersistent: true,
    ws: { workspaces: [] as Workspace[], folders: [] as Folder[], groups: [] as Group[] },
    items: [] as any[],
    data: { lines: [] as any[], terms: [], tasks: [] as any[], activity: {} },
    multi: new Set<string>(),
    profiles: null as any,
    conn: { phase: 'connected', target: { kind: 'local' } },
    nameDialog: null as any,
    broadcastTarget: null as any,
    api: api(),
    open: vi.fn(),
    notify: vi.fn(),
    progress: vi.fn(),
    refresh: vi.fn(async () => {}),
    select: vi.fn(),
    openTerm: vi.fn(async () => {}),
    resumeLine: vi.fn(async () => {}),
    patchWorkspace: vi.fn(async () => {}),
    membersOf: vi.fn(() => ['members']),
    groupItems: vi.fn(() => ['groupItems']),
    canMoveToWorkspace: vi.fn(() => false),
    selectWorkspace: vi.fn(),
    openMenu: vi.fn(),
    newSession: vi.fn(async () => ({})),
    agents: [] as any[],
  };
}

const pick = (items: MenuItem[], label: string) => {
  const found = items.find((i) => i.label === label);
  if (!found) throw new Error(`no menu item ${label} in ${items.map((i) => i.label).join(', ')}`);
  return found;
};

const labels = (items: MenuItem[]) => items.filter((i) => !i.separator).map((i) => i.label);

async function submitName(name: string) {
  await app.nameDialog.submit(name);
}

const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

const savedMuted = settings.notifyMuted;

beforeEach(() => {
  for (const k of Object.keys(app)) delete app[k];
  Object.assign(app, freshApp());
  settings.notifyMuted = {};
});

afterEach(() => {
  settings.notifyMuted = savedMuted;
  vi.restoreAllMocks();
});

describe('dialog helpers', () => {
  it('askName stores the dialog and opens it', () => {
    const submit = vi.fn();
    askName('Title', 'Label', 'value', submit);
    expect(app.nameDialog).toEqual({ title: 'Title', label: 'Label', value: 'value', submit });
    expect(app.open).toHaveBeenCalledWith('name');
  });

  it('broadcastTo targets the broadcast dialog', () => {
    broadcastTo({ kind: 'group', id: 'g1' });
    expect(app.broadcastTarget).toEqual({ kind: 'group', id: 'g1' });
    expect(app.open).toHaveBeenCalledWith('broadcast');
  });

  it.each(['hold', 'resume', 'close'] as const)('holdAll opens the hold dialog in %s mode', (mode) => {
    holdAll({ kind: 'items', keys: ['a'] }, mode);
    expect(app.broadcastTarget).toEqual({ kind: 'items', keys: ['a'] });
    expect(app.open).toHaveBeenCalledWith('holdAll', mode);
  });
});

describe('targetItems', () => {
  it('resolves workspace members', () => {
    expect(targetItems({ kind: 'workspace', id: 'w1' })).toEqual(['members']);
    expect(app.membersOf).toHaveBeenCalledWith('w1');
  });

  it('resolves group items', () => {
    expect(targetItems({ kind: 'group', id: 'g1' })).toEqual(['groupItems']);
    expect(app.groupItems).toHaveBeenCalledWith('g1');
  });

  it('keeps only the selected items, in sidebar order', () => {
    app.items = [{ key: 'a' }, { key: 'b' }, { key: 'c' }];
    expect(targetItems({ kind: 'items', keys: ['c', 'a', 'missing'] })).toEqual([{ key: 'a' }, { key: 'c' }]);
  });
});

describe('targetName', () => {
  it('names a workspace, falling back to the other sessions bucket', () => {
    app.ws.workspaces = [workspace()];
    expect(targetName({ kind: 'workspace', id: 'w1' })).toBe('Alpha');
    expect(targetName({ kind: 'workspace', id: '' })).toBe(t('ws.other'));
  });

  it('names a group, or nothing when it is gone', () => {
    app.ws.groups = [group()];
    expect(targetName({ kind: 'group', id: 'g1' })).toBe('Team');
    expect(targetName({ kind: 'group', id: 'gone' })).toBe('');
  });

  it('counts a selection', () => {
    expect(targetName({ kind: 'items', keys: ['a', 'b', 'c'] })).toBe(t('ws.selection', { n: 3 }));
  });
});

describe('rescanFolders', () => {
  it('does nothing while disconnected', async () => {
    app.connected = false;
    await rescanFolders();
    expect(app.api.discoverProjects).not.toHaveBeenCalled();
    expect(app.progress).not.toHaveBeenCalled();
  });

  it.each([
    [0, t('ws.foundNone')],
    [1, t('ws.foundOne')],
    [4, t('ws.foundMany', { n: 4 })],
  ])('reports %i new repositories after refreshing', async (added, message) => {
    app.api.discoverProjects.mockResolvedValueOnce({ added });
    await rescanFolders();
    expect(app.progress).toHaveBeenCalledWith(t('ws.rescanning'));
    expect(app.refresh).toHaveBeenCalled();
    expect(app.notify).toHaveBeenCalledWith(message);
  });

  it('reports a failed scan as an error', async () => {
    app.api.discoverProjects.mockRejectedValueOnce(new Error('offline'));
    await rescanFolders();
    expect(app.refresh).not.toHaveBeenCalled();
    expect(app.notify).toHaveBeenCalledWith('offline', 'error');
  });
});

describe('creating folders and groups', () => {
  it('creates a top-level folder from the name dialog', async () => {
    app.ws.folders = [folder()];
    await createFolder();
    expect(app.nameDialog.value).toBe('');
    await submitName('Personal');
    expect(app.api.createFolder).toHaveBeenCalledWith({ name: 'Personal', parentId: null });
    expect(app.ws.folders.map((f: Folder) => f.id)).toEqual(['f1', 'fnew']);
  });

  it('creates a subfolder and replaces a folder with the same id', async () => {
    app.ws.folders = [folder({ id: 'fnew', name: 'old' })];
    await createFolder('f1');
    await submitName('Child');
    expect(app.api.createFolder).toHaveBeenCalledWith({ name: 'Child', parentId: 'f1' });
    expect(app.ws.folders).toHaveLength(1);
    expect(app.ws.folders[0]).toMatchObject({ id: 'fnew', name: 'Child', parentId: 'f1' });
  });

  it('creates a group with the given members and clears the multi-selection', async () => {
    app.multi = new Set(['a', 'b']);
    await createGroup(['a', 'b']);
    await submitName('Pair');
    expect(app.api.createGroup).toHaveBeenCalledWith({ name: 'Pair', members: ['a', 'b'] });
    expect(app.ws.groups.map((g: Group) => g.name)).toEqual(['Pair']);
    expect(app.multi.size).toBe(0);
  });

  it('adds keys to an existing group', async () => {
    const g = group();
    app.ws.groups = [g, group({ id: 'g2', name: 'Other' })];
    app.multi = new Set(['a']);
    await addToGroup(g, ['a']);
    expect(app.api.patchGroup).toHaveBeenCalledWith('g1', { add: ['a'] });
    expect(app.ws.groups[0].patched).toBe(true);
    expect(app.ws.groups[1].patched).toBeUndefined();
    expect(app.multi.size).toBe(0);
  });

  it('keeps the selection when adding to a group fails', async () => {
    app.multi = new Set(['a']);
    app.api.patchGroup.mockRejectedValueOnce('conflict');
    await addToGroup(group(), ['a']);
    expect(app.notify).toHaveBeenCalledWith('conflict', 'error');
    expect(app.multi.size).toBe(1);
  });
});

describe('groupPicker', () => {
  it('offers only a new group when there are none', () => {
    const items = groupPicker(['a']);
    expect(items).toHaveLength(1);
    expect(items[0].label).toBe(t('grp.new'));
  });

  it('lists groups by rank then name, then a separator and new group', () => {
    app.ws.groups = [group({ id: 'c', name: 'C', rank: 2 }), group({ id: 'b', name: 'B', rank: 1 }), group({ id: 'a', name: 'A', rank: 1 })];
    const items = groupPicker(['k']);
    expect(items.map((i) => i.label)).toEqual(['A', 'B', 'C', '', t('grp.new')]);
    expect(items[3].separator).toBe(true);
  });

  it('adds to the picked group, or asks a name for a new one', async () => {
    app.ws.groups = [group()];
    const items = groupPicker(['k']);
    await items[0].run!();
    expect(app.api.patchGroup).toHaveBeenCalledWith('g1', { add: ['k'] });
    await items[2].run!();
    expect(app.open).toHaveBeenCalledWith('name');
  });
});

describe('workspaceMenu', () => {
  it('disables editing entries on a non-persistent server but keeps broadcast and hold', () => {
    app.wsPersistent = false;
    const items = workspaceMenu(workspace());
    for (const label of [t('ws.rename'), t('ws.pin'), t('ws.notifyIdle'), t('ws.archive'), t('common.delete')]) expect(pick(items, label).disabled).toBe(true);
    for (const label of [t('broadcast.title'), t('hold.all'), t('hold.resumeAll')]) expect(pick(items, label).disabled).toBeFalsy();
  });

  it('renames through the name dialog prefilled with the current name', async () => {
    const w = workspace();
    pick(workspaceMenu(w), t('ws.rename')).run!();
    expect(app.nameDialog.value).toBe('Alpha');
    await submitName('Beta');
    expect(app.patchWorkspace).toHaveBeenCalledWith('w1', { name: 'Beta' });
  });

  it('toggles pinning', () => {
    pick(workspaceMenu(workspace()), t('ws.pin')).run!();
    expect(app.patchWorkspace).toHaveBeenLastCalledWith('w1', { pinned: true });
    pick(workspaceMenu(workspace({ pinned: true })), t('ws.unpin')).run!();
    expect(app.patchWorkspace).toHaveBeenLastCalledWith('w1', { pinned: false });
  });

  it('offers moves to every other folder, naming subfolders with their parent', () => {
    app.ws.folders = [folder({ id: 'f1', name: 'Work', rank: 0 }), folder({ id: 'f2', name: 'Client', parentId: 'f1', rank: 1 }), folder({ id: 'f3', name: 'Home', rank: 2 })];
    const items = workspaceMenu(workspace({ folderId: 'f3' }));
    const moves = items.filter((i) => i.label.startsWith(t('ws.moveTo', { name: '' })) && i.label !== t('ws.moveTop'));
    expect(moves.map((i) => i.label)).toEqual([t('ws.moveTo', { name: 'Work' }), t('ws.moveTo', { name: 'Work / Client' })]);
    moves[1].run!();
    expect(app.patchWorkspace).toHaveBeenCalledWith('w1', { folderId: 'f2', pinned: false });
  });

  it('names a subfolder whose parent is missing with an empty prefix', () => {
    app.ws.folders = [folder({ id: 'f2', name: 'Orphan', parentId: 'gone' })];
    expect(labels(workspaceMenu(workspace()))).toContain(t('ws.moveTo', { name: ' / Orphan' }));
  });

  it('offers move to top level only for a workspace inside a folder', () => {
    expect(labels(workspaceMenu(workspace()))).not.toContain(t('ws.moveTop'));
    const item = pick(workspaceMenu(workspace({ folderId: 'f1' })), t('ws.moveTop'));
    item.run!();
    expect(app.patchWorkspace).toHaveBeenCalledWith('w1', { folderId: null });
  });

  it('opens broadcast and hold dialogs scoped to the workspace', () => {
    const items = workspaceMenu(workspace());
    pick(items, t('broadcast.title')).run!();
    expect(app.open).toHaveBeenLastCalledWith('broadcast');
    pick(items, t('hold.all')).run!();
    expect(app.open).toHaveBeenLastCalledWith('holdAll', 'hold');
    pick(items, t('hold.resumeAll')).run!();
    expect(app.open).toHaveBeenLastCalledWith('holdAll', 'resume');
    expect(app.broadcastTarget).toEqual({ kind: 'workspace', id: 'w1' });
  });

  it('reflects and toggles idle notifications', () => {
    const item = pick(workspaceMenu(workspace({ notifyIdle: true })), t('ws.notifyIdle'));
    expect(item.checked).toBe(true);
    item.run!();
    expect(app.patchWorkspace).toHaveBeenCalledWith('w1', { notifyIdle: false });
  });

  it('reflects and toggles the workspace mute', () => {
    const item = pick(workspaceMenu(workspace()), t('nb.mute'));
    expect(item.checked).toBe(false);
    item.run!();
    expect(settings.notifyMuted.local).toEqual(['w:w1']);
    expect(pick(workspaceMenu(workspace()), t('nb.mute')).checked).toBe(true);
  });

  it('shows the profile entry with the scoped profile name only when profiles are loaded', () => {
    expect(labels(workspaceMenu(workspace()))).not.toContain(t('profile.forWorkspace'));
    app.profiles = { profiles: [{ id: 'p1', name: 'Fast' }], scopes: { '/src/alpha': 'p1' } };
    const item = pick(workspaceMenu(workspace()), t('profile.forWorkspace'));
    expect(item.hint).toBe('Fast');
    item.run!();
    expect(app.open).toHaveBeenCalledWith('profiles', '/src/alpha');
    expect(pick(workspaceMenu(workspace({ path: '/other' })), t('profile.forWorkspace')).hint).toBeUndefined();
  });

  it('toggles archiving', () => {
    pick(workspaceMenu(workspace()), t('ws.archive')).run!();
    expect(app.patchWorkspace).toHaveBeenLastCalledWith('w1', { archived: true });
    pick(workspaceMenu(workspace({ archived: true })), t('ws.unarchive')).run!();
    expect(app.patchWorkspace).toHaveBeenLastCalledWith('w1', { archived: false });
  });

  it('deletes the workspace and drops it from the list', async () => {
    app.ws.workspaces = [workspace(), workspace({ id: 'w2' })];
    const item = pick(workspaceMenu(workspace()), t('common.delete'));
    expect(item.danger).toBe(true);
    await item.run!();
    expect(app.api.deleteWorkspace).toHaveBeenCalledWith('w1');
    expect(app.ws.workspaces.map((w: Workspace) => w.id)).toEqual(['w2']);
  });

  it('keeps the workspace when the delete fails', async () => {
    app.ws.workspaces = [workspace()];
    app.api.deleteWorkspace.mockRejectedValueOnce(new Error('busy'));
    await pick(workspaceMenu(workspace()), t('common.delete')).run!();
    expect(app.ws.workspaces).toHaveLength(1);
    expect(app.notify).toHaveBeenCalledWith('busy', 'error');
  });
});

describe('folderMenu', () => {
  it('offers a subfolder only on top-level folders', () => {
    expect(labels(folderMenu(folder()))).toContain(t('ws.newSubfolder'));
    expect(labels(folderMenu(folder({ parentId: 'f0' })))).not.toContain(t('ws.newSubfolder'));
  });

  it('creates the subfolder under this folder', async () => {
    await pick(folderMenu(folder()), t('ws.newSubfolder')).run!();
    await submitName('Sub');
    expect(app.api.createFolder).toHaveBeenCalledWith({ name: 'Sub', parentId: 'f1' });
  });

  it('renames the folder in place', async () => {
    app.ws.folders = [folder(), folder({ id: 'f2', name: 'Keep' })];
    pick(folderMenu(folder()), t('ws.rename')).run!();
    expect(app.nameDialog.value).toBe('Work');
    await submitName('Job');
    expect(app.api.patchFolder).toHaveBeenCalledWith('f1', { name: 'Job' });
    expect(app.ws.folders.map((f: Folder) => f.name)).toEqual(['Job', 'Keep']);
  });

  it('deletes the folder and refreshes', async () => {
    await pick(folderMenu(folder()), t('common.delete')).run!();
    expect(app.api.deleteFolder).toHaveBeenCalledWith('f1');
    expect(app.refresh).toHaveBeenCalled();
  });

  it('reports a failed delete without refreshing', async () => {
    app.api.deleteFolder.mockRejectedValueOnce(new Error('not empty'));
    await pick(folderMenu(folder()), t('common.delete')).run!();
    expect(app.refresh).not.toHaveBeenCalled();
    expect(app.notify).toHaveBeenCalledWith('not empty', 'error');
  });
});

describe('groupMenu', () => {
  it('scopes broadcast and hold dialogs to the group', () => {
    const items = groupMenu(group());
    pick(items, t('hold.resumeAll')).run!();
    expect(app.broadcastTarget).toEqual({ kind: 'group', id: 'g1' });
    expect(app.open).toHaveBeenLastCalledWith('holdAll', 'resume');
    pick(items, t('hold.all')).run!();
    expect(app.open).toHaveBeenLastCalledWith('holdAll', 'hold');
    pick(items, t('broadcast.title')).run!();
    expect(app.open).toHaveBeenLastCalledWith('broadcast');
  });

  it('toggles idle notifications on the server and stores the result', async () => {
    app.ws.groups = [group()];
    const item = pick(groupMenu(group()), t('ws.notifyIdle'));
    expect(item.checked).toBe(false);
    await item.run!();
    expect(app.api.patchGroup).toHaveBeenCalledWith('g1', { notifyIdle: true });
    expect(app.ws.groups[0].notifyIdle).toBe(true);
  });

  it('reports a failed idle toggle', async () => {
    app.api.patchGroup.mockRejectedValueOnce(new Error('nope'));
    await pick(groupMenu(group()), t('ws.notifyIdle')).run!();
    expect(app.notify).toHaveBeenCalledWith('nope', 'error');
  });

  it('mutes the group target', () => {
    pick(groupMenu(group()), t('nb.mute')).run!();
    expect(settings.notifyMuted.local).toEqual(['g:g1']);
  });

  it('renames the group', async () => {
    app.ws.groups = [group()];
    pick(groupMenu(group()), t('ws.rename')).run!();
    await submitName('Squad');
    expect(app.api.patchGroup).toHaveBeenCalledWith('g1', { name: 'Squad' });
    expect(app.ws.groups[0].name).toBe('Squad');
  });

  it('deletes the group', async () => {
    app.ws.groups = [group(), group({ id: 'g2' })];
    await pick(groupMenu(group()), t('common.delete')).run!();
    expect(app.api.deleteGroup).toHaveBeenCalledWith('g1');
    expect(app.ws.groups.map((g: Group) => g.id)).toEqual(['g2']);
  });

  it('keeps the group when the delete fails', async () => {
    app.ws.groups = [group()];
    app.api.deleteGroup.mockRejectedValueOnce(new Error('x'));
    await pick(groupMenu(group()), t('common.delete')).run!();
    expect(app.ws.groups).toHaveLength(1);
    expect(app.notify).toHaveBeenCalledWith('x', 'error');
  });
});

describe('otherMenu', () => {
  it('targets the sessions outside any workspace, including close all', () => {
    const items = otherMenu();
    pick(items, t('hold.closeAll')).run!();
    expect(app.broadcastTarget).toEqual({ kind: 'workspace', id: '' });
    expect(app.open).toHaveBeenLastCalledWith('holdAll', 'close');
    expect(pick(items, t('hold.closeAll')).danger).toBe(true);
    pick(items, t('broadcast.title')).run!();
    expect(app.open).toHaveBeenLastCalledWith('broadcast');
    pick(items, t('hold.all')).run!();
    expect(app.open).toHaveBeenLastCalledWith('holdAll', 'hold');
    pick(items, t('hold.resumeAll')).run!();
    expect(app.open).toHaveBeenLastCalledWith('holdAll', 'resume');
  });
});

describe('itemMenu', () => {
  const liveLine = (id: string, over: Record<string, unknown> = {}) => ({ line: { id, state: 'running' }, local: { pid: 1, tmuxPane: '%1' }, ...over });

  it('acts on the whole multi-selection when the clicked item is part of it', () => {
    app.multi = new Set(['a', 'b']);
    pick(itemMenu('a'), t('broadcast.title')).run!();
    expect(app.broadcastTarget).toEqual({ kind: 'items', keys: ['a', 'b'] });
  });

  it('acts only on the clicked item when it is outside the multi-selection', () => {
    app.multi = new Set(['a', 'b']);
    pick(itemMenu('c'), t('broadcast.title')).run!();
    expect(app.broadcastTarget).toEqual({ kind: 'items', keys: ['c'] });
  });

  it('offers mute only for a single item', () => {
    expect(labels(itemMenu('a'))).toContain(t('nb.mute'));
    app.multi = new Set(['a', 'b']);
    expect(labels(itemMenu('a'))).not.toContain(t('nb.mute'));
  });

  it('has no line actions for an item without a line', () => {
    app.items = [{ key: 'term:t', termId: 't' }];
    expect(labels(itemMenu('term:t')).slice(0, 2)).toEqual([t('tabs.rename'), t('broadcast.title')]);
  });

  it('lists attach, hold and close for a live tmux session and runs them', async () => {
    const item = { key: 'line:a', lineId: 'a' };
    app.items = [item];
    app.data.lines = [liveLine('a')];
    const items = itemMenu('line:a');
    expect(labels(items).slice(0, 3)).toEqual([t('line.attach'), t('session.action.pause'), t('line.close')]);
    expect(items[3].separator).toBe(true);
    expect(pick(items, t('line.close')).danger).toBe(true);
    await pick(items, t('line.attach')).run!();
    expect(app.openTerm).toHaveBeenCalledWith(item);
    expect(app.select).toHaveBeenCalledWith('line:a');
    pick(items, t('line.close')).run!();
    expect(app.open).toHaveBeenLastCalledWith('holdAll', 'close');
    expect(app.broadcastTarget).toEqual({ kind: 'items', keys: ['line:a'] });
  });

  it('pauses a single session with the chat header action and never opens the bulk dialog', async () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.lines = [liveLine('a')];
    app.api.holdAfterTurn = vi.fn(async () => ({ action: 'hold-pending' }));
    await pick(itemMenu('line:a'), t('session.action.pause')).run!();
    expect(app.api.holdAfterTurn).toHaveBeenCalledWith('a');
    expect(app.notify).toHaveBeenCalledWith(t('session.state.pause_pending'));
    expect(app.open).not.toHaveBeenCalled();
  });

  it('says the session is paused when it was idle', async () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.lines = [liveLine('a')];
    app.api.holdAfterTurn = vi.fn(async () => ({ action: 'hold' }));
    await pick(itemMenu('line:a'), t('session.action.pause')).run!();
    expect(app.notify).toHaveBeenCalledWith(t('chat.paused'));
  });

  it('offers to cancel a queued pause instead', async () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.lines = [liveLine('a', { local: { pid: 1, tmuxPane: '%1', holdPending: true } })];
    app.api.cancelHold = vi.fn(async () => ({}));
    const items = itemMenu('line:a');
    expect(labels(items)).not.toContain(t('session.action.pause'));
    await pick(items, t('session.action.cancelPause')).run!();
    expect(app.api.cancelHold).toHaveBeenCalledWith('a');
    expect(app.notify).toHaveBeenCalledWith(t('chat.pauseCancelled'));
  });

  it('resumes a paused session that still has its terminal, with the chat header action', async () => {
    const item = { key: 'line:a', lineId: 'a', termId: 't' };
    app.items = [item];
    app.data.lines = [liveLine('a', { line: { id: 'a', state: 'held' } })];
    const items = itemMenu('line:a');
    expect(labels(items).slice(0, 3)).toEqual([t('line.open'), t('action.resume'), t('line.close')]);
    await pick(items, t('action.resume')).run!();
    expect(app.resumeLine).toHaveBeenCalledWith(item);
    expect(app.select).toHaveBeenCalledWith('line:a');
  });

  it('reports a refused pause', async () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.lines = [liveLine('a')];
    app.api.holdAfterTurn = vi.fn(async () => {
      throw new Error('pinned');
    });
    await pick(itemMenu('line:a'), t('session.action.pause')).run!();
    expect(app.notify).toHaveBeenCalledWith(t('chat.actionFailed', { reason: 'pinned' }), 'error');
  });

  it('keeps the bulk hold dialog for a multi-selection', () => {
    app.items = [{ key: 'line:a', lineId: 'a' }, { key: 'line:b', lineId: 'b' }];
    app.data.lines = [liveLine('a'), liveLine('b')];
    app.multi = new Set(['line:a', 'line:b']);
    pick(itemMenu('line:a'), t('action.hold')).run!();
    expect(app.open).toHaveBeenLastCalledWith('holdAll', 'hold');
  });

  it('resumes a held session', async () => {
    const item = { key: 'line:a', lineId: 'a' };
    app.items = [item];
    app.data.lines = [liveLine('a', { line: { id: 'a', state: 'held' } })];
    const items = itemMenu('line:a');
    expect(labels(items).slice(0, 2)).toEqual([t('action.resume'), t('line.close')]);
    await pick(items, t('action.resume')).run!();
    expect(app.resumeLine).toHaveBeenCalledWith(item);
    expect(app.select).toHaveBeenCalledWith('line:a');
  });

  it('reports a failed line action without selecting', async () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.lines = [liveLine('a')];
    app.openTerm.mockRejectedValueOnce(new Error('tmux gone'));
    await pick(itemMenu('line:a'), t('line.attach')).run!();
    expect(app.select).not.toHaveBeenCalled();
    expect(app.notify).toHaveBeenCalledWith('tmux gone', 'error');
  });

  it('ignores a line action when the item vanished meanwhile', async () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.lines = [liveLine('a')];
    const items = itemMenu('line:a');
    app.items = [];
    await pick(items, t('line.attach')).run!();
    expect(app.openTerm).not.toHaveBeenCalled();
  });

  it('offers task moves for a single item with a task, marking the current status', async () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.tasks = [{ id: 't1', lineId: 'a', status: 'in_testing', origin: 'manual' }];
    const items = itemMenu('line:a');
    const moves = items.filter((i) => i.label.startsWith(t('tasks.moveTo', { status: '' })));
    expect(moves.map((m) => m.label)).toEqual([t('tasks.moveTo', { status: t('kanban.col.in_testing') }), t('tasks.moveTo', { status: t('kanban.col.merged') }), t('tasks.moveTo', { status: t('kanban.col.completed') })]);
    expect(moves[0]).toMatchObject({ checked: true, disabled: true });
    expect(moves[1]).toMatchObject({ checked: false, disabled: false });
    moves[1].run!();
    expect(app.api.patchTask).toHaveBeenCalledWith('t1', { status: 'merged' });
    moves[2].run!();
    expect(app.api.patchTask).toHaveBeenCalledWith('t1', { status: 'completed' });
  });

  it('offers the in progress column for session tasks', () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.tasks = [{ id: 't1', lineId: 'a', status: 'in_progress', origin: 'session' }];
    expect(labels(itemMenu('line:a'))).toContain(t('tasks.moveTo', { status: t('kanban.col.in_progress') }));
  });

  it('reports a failed task move', async () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.tasks = [{ id: 't1', lineId: 'a', status: 'in_testing', origin: 'manual' }];
    app.api.patchTask.mockRejectedValueOnce(new Error('locked'));
    pick(itemMenu('line:a'), t('tasks.moveTo', { status: t('kanban.col.completed') })).run!();
    await flush();
    expect(app.notify).toHaveBeenCalledWith('locked', 'error');
  });

  it('hides task moves for a multi-selection', () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.tasks = [{ id: 't1', lineId: 'a', status: 'in_testing', origin: 'manual' }];
    app.multi = new Set(['line:a', 'line:b']);
    expect(itemMenu('line:a').some((i) => i.label.startsWith(t('tasks.moveTo', { status: '' })))).toBe(false);
  });

  it('labels group entries as add-to and offers remove-from only for groups holding every selected key', async () => {
    app.ws.groups = [group({ id: 'g1', name: 'Both', members: ['a', 'b'] }), group({ id: 'g2', name: 'OnlyA', members: ['a'] })];
    app.multi = new Set(['a', 'b']);
    const items = itemMenu('a');
    expect(labels(items)).toEqual(expect.arrayContaining([t('grp.addTo', { name: 'Both' }), t('grp.addTo', { name: 'OnlyA' }), t('grp.new'), t('grp.removeFrom', { name: 'Both' })]));
    expect(labels(items)).not.toContain(t('grp.removeFrom', { name: 'OnlyA' }));
    await pick(items, t('grp.removeFrom', { name: 'Both' })).run!();
    expect(app.api.patchGroup).toHaveBeenCalledWith('g1', { remove: ['a', 'b'] });
    expect(app.ws.groups[0].patched).toBe(true);
  });

  it('reports a failed remove-from-group', async () => {
    app.ws.groups = [group({ members: ['a'] })];
    app.api.patchGroup.mockRejectedValueOnce(new Error('gone'));
    await pick(itemMenu('a'), t('grp.removeFrom', { name: 'Team' })).run!();
    expect(app.notify).toHaveBeenCalledWith('gone', 'error');
  });
});

describe('selectSession', () => {
  beforeEach(() => {
    app.select = vi.fn();
    app.selectedKey = null;
    app.multi = new Set();
  });

  it('opens the session and clears the multi-selection on a plain click', () => {
    app.multi = new Set(['a', 'b']);
    selectSession('c');
    expect(app.select).toHaveBeenCalledWith('c');
    expect(app.multi.size).toBe(0);
  });

  it('toggles the session in the multi-selection on Ctrl, Meta or Shift click without opening it', () => {
    app.selectedKey = 'a';
    selectSession('b', { ctrlKey: true } as MouseEvent);
    expect([...app.multi]).toEqual(['b']);
    selectSession('c', { shiftKey: true } as MouseEvent);
    selectSession('b', { metaKey: true } as MouseEvent);
    expect([...app.multi]).toEqual(['c']);
    expect(app.select).not.toHaveBeenCalled();
  });

  it('keeps the open session selected when the last toggle empties the set', () => {
    app.selectedKey = 'a';
    app.multi = new Set(['a']);
    selectSession('a', { ctrlKey: true } as MouseEvent);
    expect([...app.multi]).toEqual(['a']);
  });
});


describe('nested workspaces', () => {
  const parent = workspace({ id: 'p', name: 'endurance', path: '/home/u/endurance' });
  const nested = workspace({ id: 'n', name: 'Onboarding', path: '/home/u/endurance/20_processus/onboarding', parentId: 'p', cwd: null, folderId: null });

  beforeEach(() => {
    app.ws = { workspaces: [parent, nested], folders: [folder({ id: 'f1', name: 'Clients' })], groups: [] };
  });

  it('offers a new subject on a top-level folder workspace and opens the dialog for it', () => {
    pick(workspaceMenu(parent), t('ws.sub.new')).run!();
    expect(app.open).toHaveBeenCalledWith('workspaceNew', 'p');
  });

  it('gives a nested workspace no pin, folder move or new subject, and toggles its start at the parent root', async () => {
    const items = labels(workspaceMenu(nested));
    expect(items).not.toContain(t('ws.pin'));
    expect(items).not.toContain(t('ws.sub.new'));
    expect(items.some((l) => l.startsWith(t('ws.moveTo', { name: '' })))).toBe(false);
    const root = pick(workspaceMenu(nested), t('ws.sub.startAtRoot'));
    expect(root.checked).toBe(false);
    root.run!();
    expect(app.patchWorkspace).toHaveBeenCalledWith('n', { cwd: '/home/u/endurance' });
    const atRoot = { ...nested, cwd: '/home/u/endurance' };
    const again = pick(workspaceMenu(atRoot), t('ws.sub.startAtRoot'));
    expect(again.checked).toBe(true);
    again.run!();
    expect(app.patchWorkspace).toHaveBeenLastCalledWith('n', { cwd: null });
  });

  it('offers no start-at-root toggle on a subject without a folder', () => {
    const subject = workspace({ id: 's', name: 'Archi', path: null, kind: 'subject', cwd: '/home/u/endurance', parentId: 'p' });
    expect(labels(workspaceMenu(subject))).not.toContain(t('ws.sub.startAtRoot'));
  });

  it('starts in a nested workspace: selects it, then opens the new-session menu with where sessions start', () => {
    app.agents = [{ id: 'claude', name: 'Claude Code', installed: true }, { id: 'shell', name: 'Shell', installed: true }];
    startIn({ clientX: 3, clientY: 4 }, { ...nested, cwd: '/home/u/endurance' });
    expect(app.selectWorkspace).toHaveBeenCalledWith('n');
    const [at, title, items] = app.openMenu.mock.calls[0];
    expect(at).toEqual({ clientX: 3, clientY: 4 });
    expect(title).toBe(t('ws.sub.startHere', { name: 'Onboarding' }));
    expect(labels(items)).toContain('Claude Code');
    expect(pick(items, t('ws.sub.startsIn', { path: '/home/u/endurance' })).disabled).toBe(true);
  });

  it('lists installed agents in the new-session menu and folds the missing ones under one row', () => {
    app.agents = [
      { id: 'claude', name: 'Claude Code', installed: true },
      { id: 'amp', name: 'Amp', installed: false, installable: true, install: 'npm i -g amp' },
      { id: 'agy', name: 'Antigravity', installed: false },
      { id: 'shell', name: 'Shell', installed: true },
    ];
    app.installAgent = vi.fn(async () => {});
    const items = newSessionItems();
    expect(labels(items)).not.toContain('Amp');
    const more = pick(items, t('agents.notInstalledCount', { n: 2 }));
    expect(more.more!.map((i) => [i.label, !!i.disabled])).toEqual([
      [t('palette.installAgent', { name: 'Amp' }), false],
      ['Antigravity', true],
    ]);
    more.more![0].run!();
    expect(app.installAgent).toHaveBeenCalledWith('amp');
  });

  it('shows no not-installed row when every agent is installed', () => {
    app.agents = [{ id: 'claude', name: 'Claude Code', installed: true }];
    expect(newSessionItems().some((i) => i.more)).toBe(false);
  });

  it('starts the picked agent from the shared new-session menu and reports a failure', async () => {
    app.agents = [{ id: 'claude', name: 'Claude Code', installed: true }];
    app.newSession = vi.fn(async () => Promise.reject(new Error('daemon down')));
    pick(newSessionItems(), 'Claude Code').run!();
    await vi.waitFor(() => expect(app.notify).toHaveBeenCalledWith('daemon down', 'error'));
    expect(app.newSession).toHaveBeenCalledWith('claude');
  });

  it('offers to move a single movable session to another workspace from its menu', async () => {
    app.items = [{ key: 'line:l1', title: 'Accueil', state: 'running' }];
    app.canMoveToWorkspace = vi.fn(() => true);
    itemMenuAt('line:l1', 'Accueil', { clientX: 7, clientY: 8, preventDefault() {} } as MouseEvent);
    const items = app.openMenu.mock.calls[0][2] as MenuItem[];
    pick(items, t('ws.moveSession')).run!();
    await vi.waitFor(() => expect(moveMenu).toHaveBeenCalledWith(app, 'line:l1'));
    expect(app.openMenu).toHaveBeenLastCalledWith({ clientX: 7, clientY: 8 }, t('ws.moveSession'), [{ label: 'webshop' }]);
  });
});
