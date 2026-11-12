import { afterEach, describe, expect, it, vi } from 'vitest';
import { DaemonError, setClient } from './client';
import { MockDaemon } from './mock';
import { app } from './state.svelte';
import type { SidebarItem } from './grouping';
import { sidebarItems } from './grouping';
import type { ConnState, Folder, HostTarget, LocalLine, Project, Task, Term, Workspace } from './types';
import {
  aggregate,
  summaryParts,
  summaryText,
  allIdle,
  applyWsEvent,
  buildTree,
  EMPTY_WS,
  isGhost,
  itemWorkspaces,
  lineActions,
  paneAction,
  moveKey,
  planFolderDrop,
  folderDropRefusal,
  planWorkspaceDrop,
  rankAt,
  resolveTab,
  repoRoot,
  within,
  workspaceTabs,
  wsDropTarget,
  type WsData,
} from './workspaces';
import { stateOfGroup } from './test-fixtures';

const at = '2026-10-29T10:00:00Z';
const ws = (id: string, path: string, over: Partial<Workspace> = {}): Workspace => ({
  id,
  name: id,
  path,
  folderId: null,
  pinned: false,
  archived: false,
  auto: true,
  rank: 1,
  tabs: [],
  notifyIdle: false,
  createdAt: at,
  updatedAt: at,
  ...over,
});
const folder = (id: string, over: Partial<Folder> = {}): Folder => ({ id, name: id, parentId: null, rank: 1, collapsed: false, createdAt: at, updatedAt: at, ...over });
const item = (key: string, group: SidebarItem['group'], over: Partial<SidebarItem> = {}): SidebarItem => ({ key, kind: 'claude', group, state: stateOfGroup(group), title: key, at, ...over });
const line = (id: string, worktreePath: string, repo?: string): LocalLine => ({ line: { id, agent: 'claude', state: 'active', lastActivityAt: at, worktreePath, repo } });

describe('membership', () => {
  const home = ws('home', '/home/u');
  const webshop = ws('webshop', '/home/u/work/webshop');
  const sub = ws('sub', '/home/u/work/webshop/web');
  const projects: Project[] = [{ path: '/home/u/work/webshop', name: 'webshop', repo: 'github.com/o/webshop' }];

  it('matches on path boundaries only', () => {
    expect(within('/home/u/work/webshop', '/home/u/work/webshop')).toBe(true);
    expect(within('/home/u/work/webshop-2', '/home/u/work/webshop')).toBe(false);
    expect(within('/anything', '/')).toBe(true);
  });

  it('puts a worktree of a repository in its repository workspace, not in a workspace on HOME', () => {
    const lines = [line('wt', '/home/u/worktrees/webshop/task-1', 'github.com/o/webshop'), line('in', '/home/u/work/webshop/web/src', 'github.com/o/webshop'), line('dot', '/home/u/.config', 'local:x')];
    const items = sidebarItems(lines, [], []);
    const m = itemWorkspaces(items, { lines, terms: [], tasks: [] }, [home, webshop, sub], projects);
    expect(m.get('line:wt')).toBe('webshop');
    expect(m.get('line:in')).toBe('sub');
    expect(m.get('line:dot')).toBe('home');
  });

  it('ignores a subject (null path) when matching by path and still honours an explicit assignment to it', () => {
    // Given a subject with a null path and a cwd inside the webshop workspace
    // When membership is computed for an unassigned line and for one assigned to the subject
    // Then nothing throws, the subject matches no path, and the assignment wins
    const subject = ws('subj', null as unknown as string, { path: null, kind: 'subject', cwd: '/home/u/work/webshop' });
    const plain = line('plain', '/home/u/work/webshop/src', 'github.com/o/webshop');
    const mine = { ...line('mine', '/home/u/work/webshop/src', 'github.com/o/webshop'), local: { workspaceId: 'subj' } };
    const lines = [plain, mine];
    const items = sidebarItems(lines, [], []);
    const m = itemWorkspaces(items, { lines, terms: [], tasks: [] }, [home, subject, webshop], projects);
    expect(m.get('line:plain')).toBe('webshop');
    expect(m.get('line:mine')).toBe('subj');
  });

  it('puts a session started in a child workspace folder in the child workspace, not in the parent repository workspace', () => {
    const atRoot = { ...line('root', '/home/u/work/webshop', 'github.com/o/webshop'), local: { cwd: '/home/u/work/webshop' } };
    const inChild = { ...line('child', '/home/u/work/webshop', 'github.com/o/webshop'), local: { cwd: '/home/u/work/webshop/web/src' } };
    const away = { ...line('away', '/home/u/work/webshop', 'github.com/o/webshop'), local: { cwd: '/srv/elsewhere' } };
    const lines = [atRoot, inChild, away];
    const items = sidebarItems(lines, [], []);
    const m = itemWorkspaces(items, { lines, terms: [], tasks: [] }, [home, webshop, sub], projects);
    expect(m.get('line:root')).toBe('webshop');
    expect(m.get('line:child')).toBe('sub');
    expect(m.get('line:away')).toBe('webshop');
  });

  it('follows the task project first and the cwd for terms', () => {
    const lines = [line('t', '/tmp/elsewhere', 'github.com/o/other')];
    const tasks = [{ id: 'k', lineId: 't', project: '/home/u/work/webshop', status: 'in_progress' } as Task];
    const terms = [{ id: 's', cwd: '/home/u/notes', exited: false, cmd: ['zsh'], title: 'zsh', createdAt: at } as Term, { id: 'x', cwd: '/srv', exited: false, cmd: ['zsh'], title: 'zsh', createdAt: at } as Term];
    const items = sidebarItems(lines, terms, tasks);
    const m = itemWorkspaces(items, { lines, terms, tasks }, [home, webshop], projects);
    expect(m.get('line:t')).toBe('webshop');
    expect(m.get('term:s')).toBe('home');
    expect(m.has('term:x')).toBe(false);
  });

  it('puts sessions whose project sits inside a parent workspace in that workspace, not in Other sessions', () => {
    // Given a workspace on a parent folder, and agent worktrees outside it under ~/.overseer/worktrees
    // When membership is resolved for a task-linked line, a line with only its repo, a local repo and a task term
    // Then each one lands in the parent workspace
    const parent = ws('work', '/home/u/work');
    const projects: Project[] = [
      { path: '/home/u/work/webshop', name: 'webshop', repo: 'github.com/o/webshop' },
      { path: '/home/u/work/scratch', name: 'scratch', repo: 'local:/home/u/work/scratch' },
    ];
    const wt = '/home/u/.overseer/worktrees/webshop-1/task-1';
    const lines = [line('t', wt, 'github.com/o/other'), line('r', `${wt}b`, 'github.com/o/webshop'), line('l', `${wt}c`, 'local:/home/u/work/scratch')];
    const tasks = [{ id: 'k', lineId: 't', project: '/home/u/work/webshop', status: 'in_progress' } as Task, { id: 'q', project: '/home/u/work/webshop/', status: 'in_progress' } as Task];
    const terms = [{ id: 's', cwd: '/srv', taskId: 'q', exited: false, cmd: ['zsh'], title: 'zsh', createdAt: at } as Term];
    const items = sidebarItems(lines, terms, tasks);
    const m = itemWorkspaces(items, { lines, terms, tasks }, [parent], projects);
    expect(m.get('line:t')).toBe('work');
    expect(m.get('line:r')).toBe('work');
    expect(m.get('line:l')).toBe('work');
    expect(m.get('term:s')).toBe('work');
  });

  it('prefers the workspace of the repo project over a workspace holding the agent worktrees', () => {
    // Given a workspace on the worktrees folder and one on the folder that holds the repo's project
    // When membership is resolved for an agent worktree of that repo
    // Then the line joins the project's workspace
    const work = ws('work', '/home/u/work');
    const wts = ws('wts', '/home/u/.overseer/worktrees');
    const lines = [line('w', '/home/u/.overseer/worktrees/webshop-1/task-1', 'github.com/o/webshop')];
    const m = itemWorkspaces(sidebarItems(lines, [], []), { lines, terms: [], tasks: [] }, [work, wts], projects);
    expect(m.get('line:w')).toBe('work');
  });

  it('keeps sessions of a second clone of a repository in that clone workspace, not in the first clone', () => {
    const atk = ws('atk', '/home/u/work/atk');
    const atk2 = ws('atk2', '/home/u/work/atk2');
    const wts = ws('wts', '/home/u/.overseer/worktrees');
    const clones: Project[] = [
      { path: '/home/u/work/atk', name: 'atk', repo: 'github.com/o/pkg' },
      { path: '/home/u/work/atk2', name: 'atk2', repo: 'github.com/o/pkg' },
    ];
    const lines = [line('one', '/home/u/work/atk', 'github.com/o/pkg'), line('two', '/home/u/work/atk2', 'github.com/o/pkg'), line('wt', '/home/u/.overseer/worktrees/atk-1/feat', 'github.com/o/pkg'), line('lost', '/srv/feat', 'github.com/o/pkg')];
    const m = itemWorkspaces(sidebarItems(lines, [], []), { lines, terms: [], tasks: [] }, [atk, atk2, wts], clones);
    expect(m.get('line:one')).toBe('atk');
    expect(m.get('line:two')).toBe('atk2');
    expect(m.get('line:wt')).toBe('wts');
    expect(m.has('line:lost')).toBe(false);
  });

  it('picks the repository root of a line among its clones by path, or the only clone', () => {
    expect(repoRoot(['/a/atk', '/a/atk2'], '/a/atk2/src')).toBe('/a/atk2');
    expect(repoRoot(['/a/atk', '/a/atk2'], '/w/atk-1/feat')).toBeUndefined();
    expect(repoRoot(['/a/atk'], '/w/atk-1/feat')).toBe('/a/atk');
    expect(repoRoot(undefined, '/a/atk')).toBeUndefined();
    expect(repoRoot(['/a/atk'], undefined)).toBe('/a/atk');
  });

  it('puts an explicitly assigned line or term in its workspace before task and path rules', () => {
    // Given a task-linked line and a shell term, both assigned to the HOME workspace
    // When membership is resolved
    // Then the assignment wins over the task project and the cwd prefix
    const lines = [{ ...line('t', '/home/u/work/webshop/web', 'github.com/o/webshop'), local: { workspaceId: 'home' } }];
    const tasks = [{ id: 'k', lineId: 't', project: '/home/u/work/webshop', status: 'in_progress' } as Task];
    const terms = [{ id: 's', cwd: '/home/u/work/webshop', exited: false, cmd: ['zsh'], title: 'zsh', createdAt: at, workspaceId: 'home' } as Term];
    const items = sidebarItems(lines, terms, tasks);
    const m = itemWorkspaces(items, { lines, terms, tasks }, [home, webshop, sub], projects);
    expect(m.get('line:t')).toBe('home');
    expect(m.get('term:s')).toBe('home');
  });

  it('ignores an assignment to a workspace that no longer exists', () => {
    // Given a line and a term assigned to a deleted workspace
    // When membership is resolved
    // Then both fall back to the path rules
    const lines = [{ ...line('l', '/home/u/work/webshop/web/src', 'github.com/o/webshop'), local: { workspaceId: 'gone' } }];
    const terms = [{ id: 's', cwd: '/home/u/notes', exited: false, cmd: ['zsh'], title: 'zsh', createdAt: at, workspaceId: 'gone' } as Term];
    const items = sidebarItems(lines, terms, []);
    const m = itemWorkspaces(items, { lines, terms, tasks: [] }, [home, webshop, sub], projects);
    expect(m.get('line:l')).toBe('sub');
    expect(m.get('term:s')).toBe('home');
  });

  it('finds the sidebar workspace row under the pointer', () => {
    // Given a workspace row with a nested label, and an element outside any row
    // When the drop target is looked up from each
    // Then the row's workspace id is found from the label and nothing from outside
    const row = { getAttribute: (n: string) => (n === 'data-workspace-id' ? 'webshop' : null) };
    const el = (inRow: boolean) => ({ closest: (s: string) => (inRow && s === '[data-workspace-id]' ? row : null) }) as unknown as Element;
    expect(wsDropTarget(el(true))).toBe('webshop');
    expect(wsDropTarget(el(false))).toBeNull();
    expect(wsDropTarget(null)).toBeNull();
  });
});

describe('status aggregation', () => {
  it('ranks needs input over running over needs testing over ready and counts every agent', () => {
    const a = aggregate([
      item('a', 'running', { status: 'Editing', at: '2026-10-29T10:05:00Z' }),
      item('b', 'needs_input', { status: 'Asking', at: '2026-10-29T10:01:00Z' }),
      item('c', 'needs_testing'),
      item('d', 'paused'),
      item('e', 'stopped'),
      item('s', 'running', { kind: 'shell' }),
    ]);
    expect(a.state).toBe('needs_input');
    expect(a.counts).toEqual({ needs_input: 1, running: 1, needs_testing: 1, ready: 0, paused: 1, stopped: 1 });
    expect(a.agents).toBe(5);
    expect(a.status).toBe('Asking');
    expect(a.statusKey).toBe('b');
    expect(aggregate([item('a', 'needs_testing'), item('b', 'running')]).state).toBe('running');
    expect(aggregate([item('a', 'needs_testing'), item('b', 'pushed')]).state).toBe('needs_testing');
    expect(aggregate([item('s', 'running', { kind: 'shell' })]).state).toBe('ready');
  });

  it('never reads a workspace of paused or stopped sessions as ready', () => {
    expect(aggregate([item('a', 'paused'), item('b', 'paused')]).state).toBe('paused');
    expect(aggregate([item('a', 'stopped')]).state).toBe('stopped');
    expect(aggregate([item('a', 'paused'), item('b', 'stopped')]).state).toBe('paused');
    expect(aggregate([item('a', 'ready'), item('b', 'paused')]).state).toBe('ready');
  });

  it('ignores the stale status of paused and stopped sessions while another one is live', () => {
    const live = aggregate([item('a', 'running', { status: 'Editing' }), item('b', 'paused', { status: 'Held by you', at: '2026-10-29T11:00:00Z' })]);
    expect(live.status).toBe('Editing');
    expect(aggregate([item('b', 'paused', { status: 'Held by you' })]).status).toBe('Held by you');
  });

  it('summarises the two most urgent states with their counts', () => {
    const a = aggregate([item('a', 'running'), item('b', 'paused'), item('c', 'paused'), item('d', 'stopped')]);
    expect(summaryParts(a)).toEqual([
      { state: 'running', count: 1 },
      { state: 'paused', count: 2 },
    ]);
    expect(summaryParts(aggregate([item('a', 'ready')]))).toEqual([{ state: 'ready', count: 1 }]);
    expect(summaryParts(aggregate([]))).toEqual([{ state: 'ready', count: 0 }]);
    expect(summaryParts(undefined)).toEqual([{ state: 'ready', count: 0 }]);
  });

  it('writes the summary as "1 running · 2 paused", plain when a single session is alone', () => {
    const label = (s: string) => s.replace('_', ' ');
    expect(summaryText([{ state: 'running', count: 1 }, { state: 'paused', count: 2 }], label)).toBe('1 running · 2 paused');
    expect(summaryText([{ state: 'needs_input', count: 1 }], label)).toBe('needs input');
    expect(summaryText([{ state: 'running', count: 3 }], label)).toBe('3 running');
    expect(summaryText([{ state: 'ready', count: 0 }], label)).toBe('ready');
  });

  it('is all idle only when every live agent is at rest', () => {
    expect(allIdle([item('a', 'ready'), item('b', 'pushed'), item('h', 'paused'), item('t', 'stopped'), item('s', 'running', { kind: 'shell' })])).toBe(true);
    expect(allIdle([item('a', 'ready'), item('b', 'running')])).toBe(false);
    expect(allIdle([item('h', 'paused')])).toBe(false);
  });
});

describe('sidebar tree', () => {
  const data: WsData = {
    ...EMPTY_WS,
    folders: [folder('uni', { rank: 1, collapsed: true }), folder('alice', { rank: 2 }), folder('acme', { rank: 3 }), folder('labs', { parentId: 'alice', rank: 1 })],
    workspaces: [
      ws('root', '/home/u', { folderId: 'alice', rank: 1 }),
      ws('proxmox', '/home/u/proxmox', { folderId: 'alice', rank: 2 }),
      ws('lab1', '/home/u/lab1', { folderId: 'labs' }),
      ws('webshop', '/w/webshop', { folderId: 'acme', pinned: true }),
      ws('billing', '/w/billing', { folderId: 'acme' }),
      ws('os', '/e/os', { folderId: 'uni' }),
      ws('loose', '/x'),
      ws('old', '/old', { archived: true }),
    ],
  };
  const flat = (d: WsData, o = {}) => buildTree(d, o).nodes.map((n) => (n.type === 'folder' ? `${'  '.repeat(n.depth)}[${n.folder.name}] ${n.count}` : `${'  '.repeat(n.depth)}${n.workspace.id}`));

  it('puts pinned on top, folders in rank order with one nesting level, collapsed folders closed', () => {
    expect(buildTree(data).pinned.map((w) => w.id)).toEqual(['webshop']);
    expect(flat(data)).toEqual(['[uni] 1', '[alice] 3', '  [labs] 1', '    lab1', '  root', '  proxmox', '[acme] 1', '  billing', 'loose']);
  });

  it('hides auto workspaces whose folder is gone but keeps user ones', () => {
    const gone: WsData = { ...data, workspaces: [...data.workspaces, ws('ghost', '/gone', { auto: true, missing: true }), ws('mine', '/moved', { auto: false, missing: true })] };
    expect(isGhost(gone.workspaces.at(-2)!)).toBe(true);
    expect(isGhost(gone.workspaces.at(-1)!)).toBe(false);
    expect(flat(gone)).not.toContain('ghost');
    expect(flat(gone)).toContain('mine');
    expect(flat(gone, { showArchived: true })).not.toContain('ghost');
  });

  it('filters and shows archived on demand', () => {
    expect(flat(data, { filter: (w: Workspace) => w.id.startsWith('o') })).toEqual(['[uni] 1', '  os']);
    expect(flat(data, { showArchived: true })).toContain('old');
  });

  it('lists nested workspaces right after their parent, one level deeper, and nowhere else', () => {
    const nested: WsData = {
      ...data,
      workspaces: [
        ...data.workspaces,
        ws('api', '/w/billing/api', { parentId: 'billing', rank: 2 }),
        ws('ui', '/w/billing/ui', { parentId: 'billing', rank: 1, folderId: 'uni' }),
        ws('kit', '/w/webshop/kit', { parentId: 'webshop' }),
        ws('orphan', '/z/orphan', { parentId: 'gone' }),
      ],
    };
    expect(flat(nested)).toEqual(['[uni] 1', '[alice] 3', '  [labs] 1', '    lab1', '  root', '  proxmox', '[acme] 1', '  billing', '    ui', '    api', 'loose']);
    expect(buildTree(nested).pinned.map((w) => w.id)).toEqual(['webshop', 'kit']);
  });

  it('plans workspace drops: before a sibling, into a folder, to pinned', () => {
    const before = planWorkspaceDrop(data, 'billing', { type: 'workspace', id: 'proxmox', pos: 'before' })!;
    expect(before.folderId).toBe('alice');
    expect(before.rank).toBeGreaterThan(1);
    expect(before.rank).toBeLessThan(2);
    expect(planWorkspaceDrop(data, 'loose', { type: 'folder', id: 'uni', pos: 'inside' })).toMatchObject({ folderId: 'uni', pinned: false });
    expect(planWorkspaceDrop(data, 'billing', { type: 'pinned' })).toMatchObject({ pinned: true });
    expect(planWorkspaceDrop(data, 'webshop', { type: 'workspace', id: 'loose', pos: 'after' })).toMatchObject({ pinned: false, folderId: null });
    expect(planWorkspaceDrop(data, 'billing', { type: 'workspace', id: 'billing', pos: 'after' })).toBeNull();
  });

  it('plans folder drops and keeps nesting to one level', () => {
    expect(planFolderDrop(data, 'acme', { type: 'folder', id: 'uni', pos: 'before' })).toMatchObject({ parentId: null });
    expect(planFolderDrop(data, 'uni', { type: 'folder', id: 'acme', pos: 'inside' })).toMatchObject({ parentId: 'acme' });
    expect(planFolderDrop(data, 'alice', { type: 'folder', id: 'acme', pos: 'inside' })).toBeNull();
    expect(planFolderDrop(data, 'uni', { type: 'folder', id: 'labs', pos: 'inside' })).toBeNull();
    expect(planFolderDrop(data, 'labs', { type: 'root' })).toMatchObject({ parentId: null });
  });

  it('names the reason a folder drop is refused', () => {
    expect(folderDropRefusal(data, 'alice', { type: 'folder', id: 'acme', pos: 'inside' })).toBe('ws.folderDepth');
    expect(folderDropRefusal(data, 'uni', { type: 'folder', id: 'labs', pos: 'inside' })).toBe('ws.folderDepth');
    expect(folderDropRefusal(data, 'alice', { type: 'folder', id: 'labs', pos: 'inside' })).toBe('ws.folderDepth');
    expect(folderDropRefusal(data, 'alice', { type: 'folder', id: 'labs', pos: 'before' })).toBe('ws.folderDepth');
    expect(folderDropRefusal(data, 'uni', { type: 'pinned' })).toBe('ws.folderDropTarget');
    expect(folderDropRefusal(data, 'uni', { type: 'folder', id: 'uni', pos: 'inside' })).toBeNull();
    expect(folderDropRefusal(data, 'uni', { type: 'folder', id: 'acme', pos: 'inside' })).toBeNull();
    expect(folderDropRefusal(data, 'labs', { type: 'root' })).toBeNull();
    expect(folderDropRefusal(data, 'nope', { type: 'root' })).toBeNull();
  });

  it('computes midpoint ranks', () => {
    const sib = [
      { id: 'a', rank: 1 },
      { id: 'b', rank: 2 },
      { id: 'c', rank: 3 },
    ];
    expect(rankAt(sib, 'c', 1)).toBe(1.5);
    expect(rankAt(sib, 'a', 3)).toBeGreaterThan(3);
    expect(rankAt(sib, 'c', 0)).toBeLessThan(1);
  });
});

describe('workspace tabs', () => {
  it('keeps the saved order, follows term → line links and appends live members', () => {
    const items = [item('line:a', 'ready', { termId: 't1', at: '2026-10-29T09:00:00Z' }), item('line:b', 'running'), item('term:s', 'running', { kind: 'shell' }), item('line:h', 'paused')];
    const w = ws('w', '/w', { tabs: ['term:s', 'term:t1', 'browser:https://x.dev', 'line:gone'] });
    expect(workspaceTabs(w.tabs, items, items)).toEqual(['term:s', 'line:a', 'browser:https://x.dev', 'line:b']);
    expect(resolveTab('review:a', items)).toBe('review:a');
    expect(resolveTab('term:zz', items)).toBeNull();
  });

  it('reorders keys', () => {
    expect(moveKey(['a', 'b', 'c'], 'a', 3)).toEqual(['b', 'c', 'a']);
    expect(moveKey(['a', 'b', 'c'], 'c', 0)).toEqual(['c', 'a', 'b']);
  });
});

describe('workspace events', () => {
  it('upserts and deletes workspaces, folders and groups', () => {
    let d = applyWsEvent(EMPTY_WS, { id: '1', type: 'workspace.created', at, data: { workspace: ws('a', '/a') } });
    d = applyWsEvent(d, { id: '2', type: 'workspace.updated', at, data: { workspace: ws('a', '/a', { name: 'A' }) } });
    d = applyWsEvent(d, { id: '3', type: 'folder.created', at, data: { folder: folder('f') } });
    d = applyWsEvent(d, { id: '4', type: 'group.created', at, data: { group: { id: 'g', name: 'g', rank: 1, members: [], notifyIdle: false, createdAt: at, updatedAt: at } } });
    expect(d.workspaces.map((w) => w.name)).toEqual(['A']);
    d = applyWsEvent(d, { id: '5', type: 'folder.deleted', at, data: { id: 'f' } });
    expect(d.folders).toEqual([]);
    expect(d.groups).toHaveLength(1);
    expect(applyWsEvent(d, { id: '6', type: 'term.created', at, data: {} })).toBe(d);
  });
});

describe('Other sessions actions', () => {
  const live = (id: string, local: LocalLine['local'], state: LocalLine['line']['state'] = 'active'): LocalLine => ({
    line: { id, agent: 'claude', state, lastActivityAt: at, worktreePath: '/srv/gone' },
    local,
  });

  it('gives lines outside every workspace the same open, hold, close and resume actions', () => {
    const lines = [
      live('pane', { pid: 7, tmuxPane: '%3', busy: false, needsInput: false }),
      live('daemon', { pid: 8, busy: true, needsInput: false }),
      live('dead', { busy: false, needsInput: false, heldReason: 'lost' }, 'held'),
    ];
    const terms = [{ id: 't8', lineId: 'daemon', cwd: '/srv', exited: false, cmd: ['claude'], title: 'claude', createdAt: at } as Term];
    const items = sidebarItems(lines, terms, []);
    const m = itemWorkspaces(items, { lines, terms, tasks: [] }, [ws('home', '/home/u')], []);
    const others = items.filter((i) => !m.has(i.key));
    expect(others.map((i) => i.key).sort()).toEqual(['line:daemon', 'line:dead', 'line:pane']);
    const acts = (id: string) => lineActions(items.find((i) => i.lineId === id)!, lines.find((l) => l.line.id === id));
    expect(acts('pane')).toEqual(['attach', 'hold', 'close']);
    expect(acts('daemon')).toEqual(['open', 'hold', 'close']);
    expect(acts('dead')).toEqual(['resume', 'close']);
  });

  it('offers resume next to open for a held session whose terminal is still attached', () => {
    const held = live('held', { pid: 8, busy: false, needsInput: false }, 'held');
    const term = { id: 't9', lineId: 'held', cwd: '/srv', exited: false, cmd: ['claude'], title: 'claude', createdAt: at } as Term;
    const items = sidebarItems([held], [term], []);
    expect(lineActions(items.find((i) => i.lineId === 'held')!, held)).toEqual(['open', 'resume', 'close']);
  });

  it('offers no line actions for plain terminals', () => {
    expect(lineActions(item('term:x', 'running', { termId: 'x' }), undefined)).toEqual([]);
  });
});

class LockedDaemon extends MockDaemon {
  failures = 0;

  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    onState({ phase: 'connected', target });
  }

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    if (method === 'GET' && path.split('?')[0] === '/v1/workspaces' && this.failures > 0) {
      this.failures--;
      throw new DaemonError(500, 'database is locked');
    }
    return super.request<T>(method, path, body);
  }
}

describe('workspaces load after a daemon error', () => {
  const local: HostTarget = { kind: 'local', label: 'Local' };

  afterEach(async () => {
    await app.disconnect();
    vi.useRealTimers();
  });

  // Given the first workspaces load fails with a 500 (database locked)
  // When the daemon answers again on the backoff retry
  // Then the folders and the workspaces inside them appear without a restart
  it('retries until folders and foldered workspaces load', async () => {
    vi.useFakeTimers();
    const m = new LockedDaemon({ live: false, delayMs: 0 });
    m.failures = 2;
    setClient(m);
    app.client = m;
    await app.connect(local);
    expect(app.ws.folders).toEqual([]);
    app.selectedKey = 'line:kept';

    await vi.advanceTimersByTimeAsync(1000);
    expect(app.ws.folders).toEqual([]);
    await vi.advanceTimersByTimeAsync(2000);

    expect(app.ws.folders.map((f) => f.id)).toContain('f_acme');
    expect(app.ws.workspaces.find((w) => w.id === 'ws_overseer')?.folderId).toBe('f_acme');
    expect(app.wsPersistent).toBe(true);
    expect(app.selectedKey).toBe('line:kept');
  });

  // Given the daemon keeps failing the workspaces load
  // When the backoff schedule runs out
  // Then the app stops asking instead of polling forever
  it('stops retrying after the last backoff step', async () => {
    vi.useFakeTimers();
    const m = new LockedDaemon({ live: false, delayMs: 0 });
    m.failures = 100;
    setClient(m);
    app.client = m;
    await app.connect(local);

    await vi.advanceTimersByTimeAsync(60_000);
    const left = m.failures;
    await vi.advanceTimersByTimeAsync(600_000);

    expect(100 - left).toBe(5);
    expect(m.failures).toBe(left);
  });

  // Given a workspaces retry is pending after a failed load
  // When the user disconnects
  // Then the retry is cancelled and the daemon is not asked again
  it('cancels the pending retry on disconnect', async () => {
    vi.useFakeTimers();
    const m = new LockedDaemon({ live: false, delayMs: 0 });
    m.failures = 100;
    setClient(m);
    app.client = m;
    await app.connect(local);

    await app.disconnect();
    await vi.advanceTimersByTimeAsync(60_000);

    expect(m.failures).toBe(99);
  });
});

describe('Terminal pane action', () => {
  const live = (local: LocalLine['local'], state: LocalLine['line']['state'] = 'active'): LocalLine => ({
    line: { id: 'l1', agent: 'claude', state, lastActivityAt: at, worktreePath: '/srv/a' },
    local,
  });
  const session = (over: Partial<SidebarItem> = {}) => item('line:l1', 'running', { lineId: 'l1', ...over });

  it('marks a live session without Overseer term or tmux pane as external instead of resumable', () => {
    // Given a Claude session alive in an external terminal (pid, busy, no term, no tmux pane)
    // When the Terminal pane asks which action to offer
    // Then it gets 'external', never 'resume'
    expect(paneAction(session(), live({ pid: 4242, busy: true, needsInput: false }))).toBe('external');
  });

  it('keeps attach for a live session in a tmux pane', () => {
    // Given a live session running in a tmux pane
    // When the Terminal pane asks which action to offer
    // Then it gets 'attach'
    expect(paneAction(session(), live({ pid: 7, tmuxPane: '%3', busy: false, needsInput: false }))).toBe('attach');
  });

  it('keeps open for a session with an Overseer term', () => {
    // Given a live session that owns an Overseer term
    // When the Terminal pane asks which action to offer
    // Then it gets 'open'
    expect(paneAction(session({ termId: 't1' }), live({ pid: 8, busy: true, needsInput: false }))).toBe('open');
  });

  it('keeps resume for a held session even if a pid is still known', () => {
    // Given a held session whose daemon line still carries a pid
    // When the Terminal pane asks which action to offer
    // Then it gets 'resume'
    expect(paneAction(session(), live({ pid: 9, busy: false, needsInput: false }, 'held'))).toBe('resume');
  });

  it('keeps resume for a dead session and when the line is not loaded yet', () => {
    // Given a session without pid, and a session whose daemon line is not loaded
    // When the Terminal pane asks which action to offer
    // Then both get 'resume'
    expect(paneAction(session(), live({ busy: false, needsInput: false }))).toBe('resume');
    expect(paneAction(session(), undefined)).toBe('resume');
  });

  it('offers no pane action for an item without line', () => {
    // Given a plain terminal item
    // When the Terminal pane asks which action to offer
    // Then it gets null
    expect(paneAction(item('term:x', 'running', { termId: 'x' }), undefined)).toBeNull();
  });
});
