import { TASK_STATUSES, type Task, type TaskMessage, type TaskStatus } from './types';

export * from './kanban-core';
import { isSession, isDone, rankOf, sortColumn, byId, blockers, startable } from './kanban-core';

export interface Filter {
  search?: string;
  project?: string;
  hideSession?: boolean;
}

export const isOrganizer = (t: Task): boolean => t.origin === 'organizer' || t.origin === 'configurator';

export type Quadrant = 'do' | 'plan' | 'delegate' | 'drop' | 'none';
export const QUADRANTS: Exclude<Quadrant, 'none'>[] = ['do', 'plan', 'delegate', 'drop'];

export function quadrant(t: Pick<Task, 'urgent' | 'important'>): Quadrant {
  if (typeof t.urgent !== 'boolean' || typeof t.important !== 'boolean') return 'none';
  if (t.urgent) return t.important ? 'do' : 'delegate';
  return t.important ? 'plan' : 'drop';
}

export function quadrantFlags(q: Quadrant): { urgent: boolean | null; important: boolean | null } {
  if (q === 'none') return { urgent: null, important: null };
  return { urgent: q === 'do' || q === 'delegate', important: q === 'do' || q === 'plan' };
}

export function flagPatch(t: Pick<Task, 'urgent' | 'important'>, key: 'urgent' | 'important', value: boolean): { urgent: boolean; important: boolean } {
  return { urgent: key === 'urgent' ? value : !!t.urgent, important: key === 'important' ? value : !!t.important };
}

export interface Column {
  status: TaskStatus;
  tasks: Task[];
}

export function matches(task: Task, f: Filter): boolean {
  if (f.project && task.project !== f.project) return false;
  if (f.hideSession && isSession(task)) return false;
  const q = f.search?.trim().toLowerCase();
  if (!q) return true;
  const hay = `#${task.number} ${task.title} ${task.description} ${task.ticketKey ?? ''} ${task.project}`.toLowerCase();
  return q.split(/\s+/).every((w) => hay.includes(w));
}

export function columns(tasks: Task[], f: Filter = {}): Column[] {
  const by = new Map<TaskStatus, Task[]>(TASK_STATUSES.map((s) => [s, []]));
  for (const t of tasks) if (!isOrganizer(t) && matches(t, f)) by.get(t.status)?.push(t);
  return TASK_STATUSES.map((status) => ({ status, tasks: sortColumn(by.get(status)!) }));
}

export function matrix(tasks: Task[], f: Filter = {}): Record<Quadrant, Task[]> {
  const out: Record<Quadrant, Task[]> = { do: [], plan: [], delegate: [], drop: [], none: [] };
  for (const t of tasks) {
    if (isDone(t.status) || isOrganizer(t) || isSession(t) || !matches(t, f)) continue;
    out[quadrant(t)].push(t);
  }
  for (const q of Object.keys(out) as Quadrant[]) out[q] = sortColumn(out[q]);
  return out;
}

export function mergeMessage(list: TaskMessage[], m: TaskMessage): TaskMessage[] {
  const i = list.findIndex((x) => x.id === m.id);
  if (i < 0) return [...list, m].sort((a, b) => Date.parse(a.createdAt) - Date.parse(b.createdAt));
  return list[i].delivered === m.delivered ? list : list.map((x, j) => (j === i ? m : x));
}

export function organizing(tasks: Task[], project?: string): boolean {
  return tasks.some((t) => isOrganizer(t) && t.status === 'in_progress' && (!project || t.project === project));
}

export function autoSummary(tasks: Task[]): { running: number; queued: number } {
  let running = 0;
  let queued = 0;
  for (const t of tasks) {
    if (isOrganizer(t)) continue;
    if (t.status === 'in_progress') running++;
    else if (t.status === 'auto') queued++;
  }
  return { running, queued };
}

export function nextUp(tasks: Task[], concurrency: number): Set<string> {
  const { running } = autoSummary(tasks);
  const slots = Math.max(1, concurrency - running);
  return new Set(startable(tasks).slice(0, slots).map((t) => t.id));
}

export function chainOf(id: string, tasks: Task[]): Set<string> {
  const ids = byId(tasks);
  const down = new Map<string, string[]>();
  for (const t of tasks) for (const d of t.dependsOn ?? []) down.set(d, [...(down.get(d) ?? []), t.id]);
  const out = new Set<string>([id]);
  for (const next of [(x: string) => ids.get(x)?.dependsOn ?? [], (x: string) => down.get(x) ?? []]) {
    const stack = [id];
    while (stack.length) {
      for (const n of next(stack.pop()!)) {
        if (out.has(n)) continue;
        out.add(n);
        stack.push(n);
      }
    }
  }
  return out;
}

export function depCandidates(tasks: Task[], query: string, opts: { project?: string; exclude?: Iterable<string>; limit?: number } = {}): Task[] {
  const skip = new Set(opts.exclude ?? []);
  const q = query.trim().toLowerCase().replace(/^#/, '');
  const words = q.split(/\s+/).filter(Boolean);
  return tasks
    .filter((t) => !skip.has(t.id) && !isSession(t) && !isOrganizer(t) && (!opts.project || t.project === opts.project))
    .filter((t) => (/^\d+$/.test(q) ? String(t.number).startsWith(q) : words.every((w) => `${t.number} ${t.title}`.toLowerCase().includes(w))))
    .sort((a, b) => Number(isDone(a.status)) - Number(isDone(b.status)) || b.number - a.number)
    .slice(0, opts.limit ?? 8);
}

export type CardStatus = { kind: 'starting' | 'next' | 'blocked' | 'progress' | 'working' | 'input' | 'paused' | 'none'; pct?: number; note?: string; waiting?: number[] };

const pctOf = (p: { done: number; total: number }) => Math.round((Math.min(p.done, p.total) / p.total) * 100);

export function cardStatus(task: Task, next: Set<string>, tasks: Map<string, Task> = new Map()): CardStatus {
  const p = task.progress;
  if (isSession(task)) {
    if (isDone(task.status)) return { kind: 'none' };
    const badges = task.badges ?? [];
    if (badges.includes('needs_input')) return { kind: 'input' };
    if (badges.includes('paused')) return { kind: 'paused' };
    if (task.status !== 'in_progress') return { kind: 'none' };
    return p && p.total > 0 ? { kind: 'progress', pct: pctOf(p), note: p.note } : { kind: 'working', note: p?.note };
  }
  if (task.status === 'in_progress' && (!p || p.done === 0)) return { kind: task.termId ? 'working' : 'starting', note: p?.note };
  if (task.status === 'pending' || task.status === 'auto') {
    const waiting = blockers(task, tasks);
    if (waiting.length) return { kind: 'blocked', waiting: waiting.map((d) => d.number) };
  }
  if (task.status === 'auto' && next.has(task.id)) return { kind: 'next' };
  if (p && p.total > 0 && task.status === 'in_progress') {
    return { kind: 'progress', pct: pctOf(p), note: p.note };
  }
  return { kind: 'none' };
}

export function shiftStatus(status: TaskStatus, delta: number): TaskStatus {
  const i = TASK_STATUSES.indexOf(status);
  return TASK_STATUSES[Math.min(TASK_STATUSES.length - 1, Math.max(0, i + delta))];
}

export interface ColRect {
  status: TaskStatus;
  left: number;
  right: number;
}

export function dropTarget(x: number, rects: ColRect[]): TaskStatus | null {
  if (!rects.length) return null;
  const hit = rects.find((r) => x >= r.left && x <= r.right);
  if (hit) return hit.status;
  let best = rects[0];
  let bestDist = Infinity;
  for (const r of rects) {
    const d = x < r.left ? r.left - x : x - r.right;
    if (d < bestDist) {
      bestDist = d;
      best = r;
    }
  }
  return bestDist <= 48 ? best.status : null;
}

export type MoveAction = 'start' | 'reject' | 'patch' | 'complete';

export const SESSION_STATUSES: TaskStatus[] = ['in_progress', 'in_testing', 'merged', 'completed'];

export function canDrop(task: Task, to: TaskStatus): boolean {
  return !isSession(task) || SESSION_STATUSES.includes(to);
}

export function moveAction(from: TaskStatus, to: TaskStatus, session = false): MoveAction | null {
  if (from === to) return null;
  if (session) return to === 'completed' ? 'complete' : SESSION_STATUSES.includes(to) ? 'patch' : null;
  if (to !== 'in_progress') return 'patch';
  return from === 'in_testing' ? 'reject' : 'start';
}

export function moveTask(tasks: Task[], id: string, status: TaskStatus, now = new Date().toISOString(), rank?: number): { next: Task[]; prev: Task | null } {
  const prev = tasks.find((t) => t.id === id) ?? null;
  const reranked = rank !== undefined && rank !== prev?.rank;
  if (!prev || (prev.status === status && !reranked)) return { next: tasks, prev: null };
  return { next: tasks.map((t) => (t.id === id ? { ...t, status, updatedAt: now, ...(reranked ? { rank } : {}) } : t)), prev };
}

export function insertIndex(y: number, mids: number[]): number {
  let i = 0;
  while (i < mids.length && y > mids[i]) i++;
  return i;
}

export function rankAt(column: Task[], index: number, moving: Task): number {
  const sorted = sortColumn(column);
  const rest = sorted.filter((t) => t.id !== moving.id);
  const at = Math.max(0, Math.min(index, rest.length));
  if (!rest.length || sorted.findIndex((t) => t.id === moving.id) === at) return moving.rank;
  if (at === 0) return rankOf(rest[0]) - 1;
  if (at === rest.length) return rankOf(rest[rest.length - 1]) + 1;
  return (rankOf(rest[at - 1]) + rankOf(rest[at])) / 2;
}

export function edgeSpeed(pos: number, start: number, end: number, edge = 48, max = 18): number {
  if (end - start < edge * 2) return 0;
  if (pos < start + edge) return -Math.ceil(max * Math.min(1, (start + edge - pos) / edge));
  if (pos > end - edge) return Math.ceil(max * Math.min(1, (pos - (end - edge)) / edge));
  return 0;
}

export function revertTask(tasks: Task[], prev: Task): Task[] {
  return tasks.map((t) => (t.id === prev.id ? prev : t));
}

export function relTime(iso: string, now = Date.now()): { key: 'time.now' | 'time.m' | 'time.h' | 'time.d'; n: number } {
  const m = Math.floor((now - Date.parse(iso)) / 60_000);
  if (m < 1) return { key: 'time.now', n: 0 };
  if (m < 60) return { key: 'time.m', n: m };
  if (m < 60 * 24) return { key: 'time.h', n: Math.floor(m / 60) };
  return { key: 'time.d', n: Math.floor(m / 1440) };
}

const FAMILIES = ['opus', 'sonnet', 'haiku', 'fable'];
const cap = (w: string) => w.charAt(0).toUpperCase() + w.slice(1);

export function modelLabel(model: string): string {
  const m = model.trim().toLowerCase().replace(/\[1m\]$/, '').replace(/^claude-/, '');
  const named = m.match(/^([a-z]+)(?:-(\d+))?(?:-(\d{1,2}))?(?:-\d{8})?$/);
  if (named && FAMILIES.includes(named[1])) return [cap(named[1]), named[2] && [named[2], named[3]].filter(Boolean).join('.')].filter(Boolean).join(' ');
  const old = m.match(/^(\d+)(?:-(\d))?-([a-z]+)(?:-\d{8})?$/);
  if (old && FAMILIES.includes(old[3])) return `${cap(old[3])} ${[old[1], old[2]].filter(Boolean).join('.')}`;
  return model.trim();
}

const DEFAULT_BRANCHES = ['main', 'master'];

export interface CardMeta {
  project: boolean;
  branch: string | null;
  worktree: string | null;
}

export function cardMeta(task: Task, opts: { filtered: boolean; defaultBranch?: string }): CardMeta {
  const base = opts.defaultBranch ? [opts.defaultBranch] : DEFAULT_BRANCHES;
  const branch = task.branch && !base.includes(task.branch) ? task.branch : null;
  const wt = task.worktreePath?.replace(/\/+$/, '');
  const name = wt && wt !== task.project.replace(/\/+$/, '') ? wt.slice(wt.lastIndexOf('/') + 1) : null;
  const worktree = name && !(branch && branch.replace(/\//g, '-').endsWith(name)) ? name : null;
  return { project: !opts.filtered, branch, worktree };
}

export const CARD_PAGE = 10;
export const CARD_EST = 118;

export function shownCount(limit: number | undefined, total: number, keep = -1): number {
  return Math.min(total, Math.max(limit ?? CARD_PAGE, keep + 1));
}

export function grown(limit: number | undefined, total: number): number {
  return Math.min(total, (limit ?? CARD_PAGE) + CARD_PAGE * 2);
}

export function newTaskId(prev: ReadonlySet<string> | null, tasks: readonly { id: string }[]): string | null {
  if (!prev) return null;
  let found: string | null = null;
  for (const t of tasks) if (!prev.has(t.id)) found = t.id;
  return found;
}

export function insertBefore(column: Task[], movingId: string | null, index: number): string | null {
  let i = 0;
  for (const t of column) {
    if (t.id === movingId) continue;
    if (i++ === index) return t.id;
  }
  return null;
}

export function frameLoop(work: () => boolean, raf: (cb: () => void) => number = (cb) => requestAnimationFrame(cb), caf: (id: number) => void = (id) => cancelAnimationFrame(id)) {
  let id = 0;
  const tick = () => {
    id = 0;
    if (work()) id = raf(tick);
  };
  return {
    arm() {
      id ||= raf(tick);
    },
    cancel() {
      if (id) caf(id);
      id = 0;
    },
  };
}
