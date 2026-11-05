import type { LocalLine, Task, Term } from './types';

export type GroupId = 'needs_input' | 'needs_testing' | 'working' | 'pushed' | 'idle' | 'held';
export const GROUP_ORDER: GroupId[] = ['needs_input', 'needs_testing', 'working', 'pushed', 'idle', 'held'];

export type ItemKind = string;

export interface SidebarItem {
  key: string;
  kind: ItemKind;
  group: GroupId;
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

export function lineGroup(l: LocalLine, testing: boolean): GroupId | null {
  const { line, local } = l;
  if (line.state === 'hung-up') return null;
  if (line.state === 'held') return 'held';
  if (local?.needsInput) return 'needs_input';
  if (testing) return 'needs_testing';
  if (local?.busy) return 'working';
  if (line.prUrl) return 'pushed';
  return 'idle';
}

function lineTitle(l: LocalLine): string {
  return l.local?.title || l.line.ticketKey || l.line.branch || l.line.worktreePath?.split('/').pop() || l.line.id;
}

export function sidebarItems(lines: LocalLine[], terms: Term[], tasks: Task[], briefs: Record<string, { goal?: string; status?: string }> = {}): SidebarItem[] {
  const own = tasks.filter((t) => t.lineId && t.origin !== 'session');
  const testing = new Set(own.filter((t) => t.status === 'in_testing').map((t) => t.lineId));
  const taskByLine = new Map(own.map((t) => [t.lineId!, t]));
  const termByLine = new Map(terms.filter((t) => t.lineId && !t.exited).map((t) => [t.lineId!, t.id]));
  const items: SidebarItem[] = [];
  for (const l of lines) {
    const group = lineGroup(l, testing.has(l.line.id));
    if (!group) continue;
    const task = taskByLine.get(l.line.id);
    items.push({
      key: `line:${l.line.id}`,
      kind: l.line.agent,
      group,
      title: l.local?.titleOverride || (task?.title ?? lineTitle(l)),
      renamed: !!l.local?.titleOverride,
      goal: briefs[l.line.id]?.goal ?? task?.description,
      status: briefs[l.line.id]?.status,
      at: l.line.lastActivityAt,
      lineId: l.line.id,
      termId: termByLine.get(l.line.id) ?? l.local?.termId,
      taskId: task?.id,
    });
  }
  for (const t of terms) {
    if (t.lineId) continue;
    items.push({
      key: `term:${t.id}`,
      kind: t.agent ?? 'shell',
      group: t.exited ? 'idle' : 'working',
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

const ITEM_FIELDS: (keyof SidebarItem)[] = ['key', 'kind', 'group', 'title', 'goal', 'status', 'at', 'lineId', 'termId', 'taskId'];

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

export function newestFirst(a: { at: string }, b: { at: string }): number {
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
