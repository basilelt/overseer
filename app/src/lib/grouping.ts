import { FEEDBACK_TITLE, isFeedbackTerm } from './feedback-watch';
import { isEnded, sessionState, type SessionState } from './session-state';
import type { LocalLine, Task, Term } from './types';

export type GroupId = 'needs_input' | 'needs_testing' | 'running' | 'pushed' | 'ready' | 'paused' | 'stopped';
export const GROUP_ORDER: GroupId[] = ['needs_input', 'needs_testing', 'running', 'pushed', 'ready', 'paused', 'stopped'];

export type ItemKind = string;

export interface SidebarItem {
  key: string;
  kind: ItemKind;
  group: GroupId;
  /** The process state shown on the row; the group may be a task or PR bucket on top of it. */
  state: SessionState;
  title: string;
  /** The title is the user's custom one, not the auto title. */
  renamed?: boolean;
  goal?: string;
  status?: string;
  at: string;
  lineId?: string;
  termId?: string;
  taskId?: string;
}

export interface Group {
  id: GroupId;
  items: SidebarItem[];
}

// needs_testing and pushed sit on top of the process state; paused and stopped lines are never urgent.
export function lineGroup(l: LocalLine, testing: boolean, termId?: string | null): GroupId | null {
  if (isEnded(l)) return null;
  const state = sessionState(l, termId).state;
  if (state === 'paused') return 'paused';
  if (l.line.state === 'held') return 'stopped';
  if (state === 'attention') return 'needs_input';
  if (testing) return 'needs_testing';
  if (state === 'running') return 'running';
  if (l.line.prUrl) return 'pushed';
  return state === 'closed' ? 'stopped' : 'ready';
}

/** The session has no live process: paused on purpose or stopped. */
export const isOff = (i: Pick<SidebarItem, 'group'>): boolean => i.group === 'paused' || i.group === 'stopped';

function lineTitle(l: LocalLine): string {
  return l.local?.title || l.line.ticketKey || l.line.branch || l.line.worktreePath?.split('/').pop() || l.line.id;
}

export function sidebarItems(lines: LocalLine[], terms: Term[], tasks: Task[], briefs: Record<string, { goal?: string; status?: string }> = {}): SidebarItem[] {
  const own = tasks.filter((t) => t.lineId && t.origin !== 'session');
  const testing = new Set(own.filter((t) => t.status === 'in_testing').map((t) => t.lineId));
  const taskByLine = new Map(own.map((t) => [t.lineId!, t]));
  const termByLine = new Map(terms.filter((t) => t.lineId && !t.exited).map((t) => [t.lineId!, t.id]));
  const feedbackLines = new Set(terms.filter((t) => t.lineId && isFeedbackTerm(t)).map((t) => t.lineId!));
  const items: SidebarItem[] = [];
  for (const l of lines) {
    const termId = termByLine.get(l.line.id) ?? l.local?.termId ?? undefined;
    const group = lineGroup(l, testing.has(l.line.id), termId);
    if (!group) continue;
    const task = taskByLine.get(l.line.id);
    items.push({
      key: `line:${l.line.id}`,
      kind: l.line.agent,
      group,
      state: sessionState(l, termId).state,
      title: l.local?.titleOverride || (task?.title ?? (feedbackLines.has(l.line.id) ? FEEDBACK_TITLE : lineTitle(l))),
      renamed: !!l.local?.titleOverride,
      goal: briefs[l.line.id]?.goal ?? task?.description,
      status: briefs[l.line.id]?.status,
      at: l.line.lastActivityAt,
      lineId: l.line.id,
      termId,
      taskId: task?.id,
    });
  }
  for (const t of terms) {
    if (t.lineId) continue;
    items.push({
      key: `term:${t.id}`,
      kind: t.agent ?? 'shell',
      group: t.exited ? 'stopped' : 'running',
      state: t.exited ? 'closed' : 'running',
      title: t.titleOverride || t.title || t.cmd.join(' '),
      renamed: !!t.titleOverride,
      goal: t.cwd,
      at: t.createdAt,
      termId: t.id,
      taskId: t.taskId ?? undefined,
    });
  }
  return items;
}

const ITEM_FIELDS: (keyof SidebarItem)[] = ['key', 'kind', 'group', 'state', 'title', 'goal', 'status', 'at', 'lineId', 'termId', 'taskId'];

function sameItem(a: SidebarItem, b: SidebarItem): boolean {
  for (const f of ITEM_FIELDS) if (a[f] !== b[f]) return false;
  return true;
}

export function stableItems(prev: SidebarItem[], next: SidebarItem[]): SidebarItem[] {
  if (!prev.length) return next.length ? next : prev;
  const old = new Map(prev.map((i) => [i.key, i]));
  let same = prev.length === next.length;
  const out = next.map((it, i) => {
    const was = old.get(it.key);
    const keep = was && sameItem(was, it) ? was : it;
    if (keep !== prev[i]) same = false;
    return keep;
  });
  return same ? prev : out;
}

// An unreadable date (not starting with a year digit) is the oldest, as timeBucket files it.
const dated = (s: string): boolean => s >= '0' && s < ':';

export function newestFirst(a: { at: string }, b: { at: string }): number {
  if (!dated(a.at) || !dated(b.at)) return dated(a.at) === dated(b.at) ? 0 : dated(a.at) ? -1 : 1;
  return a.at < b.at ? 1 : a.at > b.at ? -1 : 0;
}

export function groupItems(items: SidebarItem[]): Group[] {
  const byGroup = new Map<GroupId, SidebarItem[]>(GROUP_ORDER.map((g) => [g, []]));
  for (const it of items) byGroup.get(it.group)!.push(it);
  return GROUP_ORDER.map((id) => ({
    id,
    items: byGroup.get(id)!.sort(newestFirst),
  }));
}
