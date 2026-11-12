import { isOff, newestFirst, type GroupId, type SidebarItem } from '../session/grouping';
import { isKnown } from '../tabs/known-tabs';
import type { Snapshot } from '../reduce';
import type { DaemonEvent, Folder, Group, LocalLine, Project, Task, Term, Workspace } from '../types';
import { isAgentItem } from '../agents';

export interface WsData {
  workspaces: Workspace[];
  folders: Folder[];
  groups: Group[];
}

export const EMPTY_WS: WsData = { workspaces: [], folders: [], groups: [] };

export type WsState = 'needs_input' | 'running' | 'needs_testing' | 'ready' | 'paused' | 'stopped';
export const WS_STATE_ORDER: WsState[] = ['needs_input', 'running', 'needs_testing', 'ready', 'paused', 'stopped'];

export interface Aggregate {
  state: WsState;
  counts: Record<WsState, number>;
  agents: number;
  status?: string;
  statusKey?: string;
  at?: string;
}

export function within(path: string, root: string): boolean {
  const r = root.replace(/\/+$/, '') || '/';
  return path === r || path.startsWith(r === '/' ? '/' : `${r}/`);
}

interface PrefixIndex {
  roots: { w: Workspace; dir: string }[];
  hits: Map<string, Workspace | undefined>;
}

const prefixIndexes = new WeakMap<Workspace[], PrefixIndex>();

function prefixIndex(workspaces: Workspace[]): PrefixIndex {
  let ix = prefixIndexes.get(workspaces);
  if (!ix) {
    ix = { roots: workspaces.filter((w) => w.path != null).map((w) => ({ w, dir: w.path!.replace(/\/+$/, '') || '/' })), hits: new Map() };
    prefixIndexes.set(workspaces, ix);
  }
  return ix;
}

function longestPrefix(path: string | undefined, workspaces: Workspace[]): Workspace | undefined {
  if (!path) return undefined;
  const ix = prefixIndex(workspaces);
  if (ix.hits.has(path)) return ix.hits.get(path);
  let best: Workspace | undefined;
  let bestLen = -1;
  for (const { w, dir } of ix.roots) {
    const inside = path === dir || path.startsWith(dir === '/' ? '/' : `${dir}/`);
    if (inside && w.path!.length > bestLen) {
      best = w;
      bestLen = w.path!.length;
    }
  }
  if (ix.hits.size > 4096) ix.hits.clear();
  ix.hits.set(path, best);
  return best;
}

export interface MemberIndex {
  byId: Map<string, Workspace>;
  repoRoots: Map<string, string[]>;
  taskByLine: Map<string, Task>;
  taskById: Map<string, Task>;
  workspaces: Workspace[];
}

const memberIndexes = new WeakMap<Workspace[], WeakMap<Project[], WeakMap<Task[], MemberIndex>>>();

export function memberIndex(workspaces: Workspace[], projects: Project[], tasks: Task[]): MemberIndex {
  let byProjects = memberIndexes.get(workspaces);
  if (!byProjects) memberIndexes.set(workspaces, (byProjects = new WeakMap()));
  let byTasks = byProjects.get(projects);
  if (!byTasks) byProjects.set(projects, (byTasks = new WeakMap()));
  let ix = byTasks.get(tasks);
  if (!ix) byTasks.set(tasks, (ix = buildMemberIndex(workspaces, projects, tasks)));
  return ix;
}

function buildMemberIndex(workspaces: Workspace[], projects: Project[], tasks: Task[]): MemberIndex {
  const repoRoots = new Map<string, string[]>();
  for (const p of projects) if (p.repo) repoRoots.set(p.repo, [...(repoRoots.get(p.repo) ?? []), p.path]);
  return {
    byId: new Map(workspaces.map((w) => [w.id, w])),
    repoRoots,
    taskByLine: new Map(tasks.filter((t) => t.lineId && t.origin !== 'session').map((t) => [t.lineId!, t])),
    taskById: new Map(tasks.map((t) => [t.id, t])),
    workspaces,
  };
}

export function repoRoot(clones: string[] | undefined, path: string | undefined): string | undefined {
  if (!clones) return undefined;
  let best: string | undefined;
  for (const c of clones) if (path && within(path, c) && (!best || c.length > best.length)) best = c;
  return best ?? (clones.length === 1 ? clones[0] : undefined);
}

export function lineWorkspace(l: LocalLine, ix: MemberIndex): Workspace | undefined {
  const assigned = l.local?.workspaceId && ix.byId.get(l.local.workspaceId);
  if (assigned) return assigned;
  const task = ix.taskByLine.get(l.line.id);
  const byTask = task && longestPrefix(task.project, ix.workspaces);
  if (byTask) return byTask;
  const wt = l.line.worktreePath;
  const cwd = l.local?.cwd;
  const path = cwd && wt && within(cwd, wt) ? cwd : wt || cwd;
  const p = longestPrefix(path, ix.workspaces);
  const r = longestPrefix(l.line.repo ? repoRoot(ix.repoRoots.get(l.line.repo), path) : undefined, ix.workspaces);
  if (r && (!p || !within(p.path!, r.path!))) return r;
  return p;
}

export function termWorkspace(t: Term, ix: MemberIndex): Workspace | undefined {
  const assigned = t.workspaceId && ix.byId.get(t.workspaceId);
  if (assigned) return assigned;
  const task = t.taskId ? ix.taskById.get(t.taskId) : undefined;
  return (task && longestPrefix(task.project, ix.workspaces)) || longestPrefix(t.cwd, ix.workspaces);
}

export function itemWorkspaces(items: SidebarItem[], snap: Pick<Snapshot, 'lines' | 'terms' | 'tasks'>, workspaces: Workspace[], projects: Project[]): Map<string, string> {
  const ix = memberIndex(workspaces, projects, snap.tasks);
  const lines = new Map(snap.lines.map((l) => [l.line.id, l]));
  const terms = new Map(snap.terms.map((t) => [t.id, t]));
  const out = new Map<string, string>();
  for (const it of items) {
    const l = it.lineId ? lines.get(it.lineId) : undefined;
    const t = !l && it.termId ? terms.get(it.termId) : undefined;
    const w = l ? lineWorkspace(l, ix) : t ? termWorkspace(t, ix) : undefined;
    if (w) out.set(it.key, w.id);
  }
  return out;
}

export function sameMap<K, V>(a: Map<K, V>, b: Map<K, V>): boolean {
  if (a === b) return true;
  if (a.size !== b.size) return false;
  for (const [k, v] of a) if (b.get(k) !== v || (v === undefined && !b.has(k))) return false;
  return true;
}

const liveLists = new WeakMap<Workspace[], Workspace[]>();

export function liveWorkspaces(workspaces: Workspace[]): Workspace[] {
  let live = liveLists.get(workspaces);
  if (!live) liveLists.set(workspaces, (live = workspaces.filter((w) => !w.archived)));
  return live;
}

export function wsState(group: GroupId): WsState {
  return group === 'pushed' ? 'ready' : group;
}

export function aggregate(items: SidebarItem[]): Aggregate {
  const counts: Record<WsState, number> = { needs_input: 0, running: 0, needs_testing: 0, ready: 0, paused: 0, stopped: 0 };
  const agentsOnly = items.filter((i) => isAgentItem(i));
  let latest: SidebarItem | undefined;
  for (const it of agentsOnly) {
    counts[wsState(it.group)]++;
    if (it.status && !isOff(it) && (!latest || it.at > latest.at)) latest = it;
  }
  const agents = agentsOnly.length;
  const state = WS_STATE_ORDER.find((s) => counts[s] > 0) ?? 'ready';
  let top: SidebarItem | undefined;
  for (const i of agentsOnly) if (wsState(i.group) === state && (!top || i.at > top.at)) top = i;
  const status = state === 'needs_input' || state === 'needs_testing' ? (top?.status ?? latest?.status) : (latest?.status ?? top?.status);
  let at: string | undefined;
  for (const i of items) if (at === undefined || i.at > at) at = i.at;
  return { state, counts, agents, status, statusKey: top?.key, at };
}

export function summaryParts(agg: Aggregate | undefined): { state: WsState; count: number }[] {
  const parts = agg ? WS_STATE_ORDER.filter((s) => agg.counts[s] > 0).map((s) => ({ state: s, count: agg.counts[s] })) : [];
  return parts.length ? parts.slice(0, 2) : [{ state: 'ready', count: 0 }];
}

export function summaryText(parts: { state: WsState; count: number }[], label: (s: WsState) => string): string {
  if (parts.length === 1 && parts[0]!.count <= 1) return label(parts[0]!.state);
  return parts.map((p) => `${p.count} ${label(p.state).toLowerCase()}`).join(' · ');
}

export function allIdle(items: SidebarItem[]): boolean {
  const live = items.filter((i) => !isOff(i) && isAgentItem(i));
  return live.length > 0 && live.every((i) => i.group === 'ready' || i.group === 'pushed');
}

export function byRank<T extends { rank: number; name: string }>(a: T, b: T): number {
  return a.rank - b.rank || a.name.localeCompare(b.name);
}

export type TreeNode =
  | { type: 'folder'; folder: Folder; depth: number; count: number }
  | { type: 'workspace'; workspace: Workspace; depth: number };

export function isGhost(w: Workspace): boolean {
  return w.auto && !!w.missing;
}

export function buildTree(data: WsData, opts: { showArchived?: boolean; filter?: (w: Workspace) => boolean } = {}): { pinned: Workspace[]; nodes: TreeNode[] } {
  const keep = (w: Workspace) => !isGhost(w) && (opts.showArchived || !w.archived) && (!opts.filter || opts.filter(w));
  const all = data.workspaces.filter(keep);
  const tops = new Set(all.filter((w) => !w.parentId).map((w) => w.id));
  const visible = all.filter((w) => !w.parentId);
  const kidsOf = (id: string) => all.filter((w) => w.parentId === id && tops.has(id)).sort(byRank);
  const withKids = (list: Workspace[]) => list.flatMap((w) => [w, ...kidsOf(w.id)]);
  const pinned = withKids(visible.filter((w) => w.pinned).sort(byRank));
  const rest = visible.filter((w) => !w.pinned);
  const folderIds = new Set(data.folders.map((f) => f.id));
  const inFolder = (id: string | null) => rest.filter((w) => (id === null ? !w.folderId || !folderIds.has(w.folderId) : w.folderId === id)).sort(byRank);
  const top = data.folders.filter((f) => !f.parentId || !folderIds.has(f.parentId)).sort(byRank);
  const deep = (f: Folder): number => inFolder(f.id).length + data.folders.filter((c) => c.parentId === f.id).reduce((n, c) => n + inFolder(c.id).length, 0);
  const nodes: TreeNode[] = [];
  const pushWs = (w: Workspace, depth: number) => {
    nodes.push({ type: 'workspace', workspace: w, depth });
    for (const c of kidsOf(w.id)) nodes.push({ type: 'workspace', workspace: c, depth: depth + 1 });
  };
  const pushFolder = (f: Folder, depth: number) => {
    const count = deep(f);
    if (opts.filter && !count) return;
    nodes.push({ type: 'folder', folder: f, depth, count });
    if (f.collapsed && !opts.filter) return;
    if (depth === 0) for (const c of data.folders.filter((c) => c.parentId === f.id).sort(byRank)) pushFolder(c, 1);
    for (const w of inFolder(f.id)) pushWs(w, depth + 1);
  };
  for (const f of top) pushFolder(f, 0);
  for (const w of inFolder(null)) pushWs(w, 0);
  return { pinned, nodes };
}

export function rankBetween(before: number | undefined, after: number | undefined): number {
  if (before === undefined && after === undefined) return Date.now();
  if (before === undefined) return after! - 1024;
  if (after === undefined) return before + 1024;
  return (before + after) / 2;
}

export function rankAt<T extends { id: string; rank: number }>(siblings: T[], movingId: string, index: number): number {
  const rest = siblings.filter((s) => s.id !== movingId);
  const i = Math.max(0, Math.min(index, rest.length));
  return rankBetween(rest[i - 1]?.rank, rest[i]?.rank);
}

export type DropPos = 'before' | 'after' | 'inside';
export type DropTarget = { type: 'workspace'; id: string; pos: DropPos } | { type: 'folder'; id: string; pos: DropPos } | { type: 'pinned' } | { type: 'root' };

function placeAmong<T extends { id: string; rank: number; name: string }>(siblings: T[], movingId: string, anchorId: string, pos: DropPos): number {
  const list = siblings.filter((s) => s.id !== movingId).sort(byRank);
  const i = list.findIndex((s) => s.id === anchorId);
  const at = i < 0 ? list.length : pos === 'after' ? i + 1 : i;
  return rankBetween(list[at - 1]?.rank, list[at]?.rank);
}

export function planWorkspaceDrop(d: WsData, id: string, target: DropTarget): Partial<Workspace> | null {
  const w = d.workspaces.find((x) => x.id === id);
  if (!w) return null;
  const tail = (list: Workspace[]) => rankBetween(list.filter((x) => x.id !== id).sort(byRank).at(-1)?.rank, undefined);
  switch (target.type) {
    case 'workspace': {
      const anchor = d.workspaces.find((x) => x.id === target.id);
      if (!anchor || anchor.id === id) return null;
      const siblings = d.workspaces.filter((x) => !x.archived && x.pinned === anchor.pinned && (anchor.pinned || (x.folderId ?? null) === (anchor.folderId ?? null)));
      const rank = placeAmong(siblings, id, anchor.id, target.pos === 'inside' ? 'after' : target.pos);
      return anchor.pinned ? { pinned: true, rank } : { pinned: false, folderId: anchor.folderId ?? null, rank };
    }
    case 'folder': {
      if (!d.folders.some((f) => f.id === target.id)) return null;
      return { pinned: false, folderId: target.id, rank: tail(d.workspaces.filter((x) => !x.pinned && x.folderId === target.id)) };
    }
    case 'pinned':
      return { pinned: true, rank: tail(d.workspaces.filter((x) => x.pinned)) };
    case 'root':
      return { pinned: false, folderId: null, rank: tail(d.workspaces.filter((x) => !x.pinned && !x.folderId)) };
  }
}

export function planFolderDrop(d: WsData, id: string, target: DropTarget): Partial<Folder> | null {
  const f = d.folders.find((x) => x.id === id);
  if (!f || target.type === 'workspace' || target.type === 'pinned') return null;
  const hasChildren = d.folders.some((x) => x.parentId === id);
  if (target.type === 'root') return { parentId: null, rank: rankBetween(d.folders.filter((x) => !x.parentId && x.id !== id).sort(byRank).at(-1)?.rank, undefined) };
  const anchor = d.folders.find((x) => x.id === target.id);
  if (!anchor || anchor.id === id) return null;
  if (target.pos === 'inside') {
    if (anchor.parentId || hasChildren) return null;
    return { parentId: anchor.id, rank: rankBetween(d.folders.filter((x) => x.parentId === anchor.id).sort(byRank).at(-1)?.rank, undefined) };
  }
  const parentId = anchor.parentId ?? null;
  if (parentId && (hasChildren || parentId === id)) return null;
  return { parentId, rank: placeAmong(d.folders.filter((x) => (x.parentId ?? null) === parentId), id, anchor.id, target.pos) };
}

export function folderDropRefusal(d: WsData, id: string, target: DropTarget): 'ws.folderDropTarget' | 'ws.folderDepth' | null {
  if (!d.folders.some((x) => x.id === id)) return null;
  if (target.type === 'workspace' || target.type === 'pinned') return 'ws.folderDropTarget';
  if (target.type === 'root') return null;
  const anchor = d.folders.find((x) => x.id === target.id);
  if (!anchor || anchor.id === id) return null;
  const hasChildren = d.folders.some((x) => x.parentId === id);
  if (target.pos === 'inside') return anchor.parentId || hasChildren ? 'ws.folderDepth' : null;
  return anchor.parentId && (hasChildren || anchor.parentId === id) ? 'ws.folderDepth' : null;
}

export function moveKey(keys: string[], key: string, to: number): string[] {
  const from = keys.indexOf(key);
  if (from < 0) return keys;
  const next = keys.filter((k) => k !== key);
  next.splice(Math.max(0, Math.min(to > from ? to - 1 : to, next.length)), 0, key);
  return next;
}

export function workspaceTabs(saved: string[], members: SidebarItem[], items: SidebarItem[], known?: Set<string>): string[] {
  const resolved = saved.map((k) => resolveTab(k, items)).filter((k): k is string => !!k);
  const seen = new Set(resolved);
  const live = members.filter((m) => !isOff(m) && !seen.has(m.key) && !(known && isKnown(m, known))).sort((a, b) => newestFirst(b, a));
  return [...new Set([...resolved, ...live.map((m) => m.key)])];
}

export function resolveTab(key: string, items: SidebarItem[]): string | null {
  if (!key.startsWith('line:') && !key.startsWith('term:')) return key;
  if (items.some((i) => i.key === key)) return key;
  if (key.startsWith('term:')) {
    const id = key.slice(5);
    return items.find((i) => i.termId === id)?.key ?? null;
  }
  return null;
}

export function upsert<T extends { id: string }>(list: T[], item: T): T[] {
  const i = list.findIndex((x) => x.id === item.id);
  if (i < 0) return [...list, item];
  const next = list.slice();
  next[i] = item;
  return next;
}

export function applyWsEvent(d: WsData, e: DaemonEvent): WsData {
  const [kind, op] = e.type.split('.') as [string, string];
  const field = kind === 'workspace' ? 'workspaces' : kind === 'folder' ? 'folders' : kind === 'group' ? 'groups' : null;
  if (!field) return d;
  if (op === 'deleted') {
    const id = e.data?.id;
    return id ? { ...d, [field]: (d[field] as { id: string }[]).filter((x) => x.id !== id) } : d;
  }
  const item = e.data?.[kind];
  if (!item?.id) return d;
  return { ...d, [field]: upsert(d[field] as { id: string }[], item) };
}

export function synthesize(projects: Project[]): Workspace[] {
  const at = new Date(0).toISOString();
  return projects.map((p, i) => ({
    id: `p:${p.path}`,
    name: p.name,
    path: p.path,
    folderId: null,
    pinned: false,
    archived: false,
    auto: true,
    rank: i,
    tabs: [],
    notifyIdle: false,
    createdAt: at,
    updatedAt: at,
  }));
}

export type LineAction = 'open' | 'attach' | 'resume' | 'hold' | 'close';

const lineAlive = (line: LocalLine) => line.local?.pid != null && line.line.state !== 'held' && line.line.state !== 'hung-up';

export function lineActions(item: SidebarItem, line: LocalLine | undefined): LineAction[] {
  if (!item.lineId || !line) return [];
  const alive = lineAlive(line);
  const out: LineAction[] = [];
  if (item.termId) {
    out.push('open');
    if (!alive) out.push('resume');
  } else if (alive && line.local?.tmuxPane) out.push('attach');
  else if (!alive) out.push('resume');
  if (alive && (line.local?.tmuxPane || item.termId) && !line.local?.pinned) out.push('hold');
  out.push('close');
  return out;
}

export type PaneAction = 'open' | 'attach' | 'resume' | 'external';

// A live session with neither an Overseer term nor a tmux pane runs in another terminal:
// resuming it would start a second writer on the same transcript.
export function paneAction(item: SidebarItem, line: LocalLine | undefined): PaneAction | null {
  if (!item.lineId) return null;
  const acts = lineActions(item, line);
  if (acts.includes('open')) return 'open';
  if (acts.includes('attach')) return 'attach';
  if (line && lineAlive(line)) return 'external';
  return 'resume';
}

export function wsDropTarget(el: Element | null): string | null {
  return el?.closest('[data-workspace-id]')?.getAttribute('data-workspace-id') ?? null;
}
