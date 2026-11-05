import type { DaemonClient } from './client';
import type { LocalLine, Project, Task, Workspace } from './types';
import { lineWorkspace, liveWorkspaces, memberIndex } from './workspaces';

export type ActionKind = 'create_folder' | 'create_workspace' | 'move_workspace' | 'create_group' | 'hold' | 'close' | 'pin' | 'archive_workspace' | 'set_task_status' | 'track';

export interface Action {
  id: string;
  kind: ActionKind;
  label?: string;
  name?: string;
  ref?: string;
  path?: string;
  folder?: string;
  pinned?: boolean;
  workspaceId?: string;
  lines?: string[];
  lineId?: string;
  taskId?: string;
  status?: string;
}

export type Why = 'dirty' | 'todos' | 'recent';

export interface PickUpItem {
  lineId: string;
  summary: string;
  why: Why;
  title?: string;
  branch?: string;
  repo?: string;
  workspaceId?: string;
  edited?: number;
  todos?: number;
}

export interface Result {
  actionId: string;
  kind: ActionKind;
  status: 'done' | 'skipped' | 'failed';
  detail?: string;
}

export interface Proposal {
  id: string;
  version: number;
  createdAt: string;
  status: 'pending_review' | 'applied' | 'dismissed';
  source: 'agent' | 'deterministic';
  taskId?: string;
  summary: string;
  actions: Action[];
  pickUp: PickUpItem[];
  reasons: Record<string, string>;
  results?: Result[];
}

export type Section = 'layout' | 'cleanup' | 'tasks';

export const SECTIONS: { id: Section; kinds: ActionKind[] }[] = [
  { id: 'layout', kinds: ['create_folder', 'create_workspace', 'move_workspace', 'create_group', 'pin', 'archive_workspace'] },
  { id: 'cleanup', kinds: ['hold', 'close'] },
  { id: 'tasks', kinds: ['set_task_status', 'track'] },
];

export const NUDGE_THRESHOLD = 15;
export const AGENT_WAIT_MS = 5 * 60 * 1000;
const RECENT_MS = 7 * 24 * 3600 * 1000;

export function sectionOf(kind: ActionKind): Section {
  return SECTIONS.find((s) => s.kinds.includes(kind))?.id ?? 'layout';
}

export function grouped(actions: Action[]): { id: Section; actions: Action[] }[] {
  return SECTIONS.map((s) => ({ id: s.id, actions: actions.filter((a) => sectionOf(a.kind) === s.id) })).filter((s) => s.actions.length);
}

export function defaultChecked(a: Action): boolean {
  return a.kind !== 'close';
}

export function initialChecks(actions: Action[]): Record<string, boolean> {
  return Object.fromEntries(actions.map((a) => [a.id, defaultChecked(a)]));
}

export function checkedIds(actions: Action[], checked: Record<string, boolean>): string[] {
  return actions.filter((a) => checked[a.id]).map((a) => a.id);
}

export function unplacedCount(lines: LocalLine[], workspaces: Workspace[], projects: Project[], tasks: Task[], now = Date.now()): number {
  const ix = memberIndex(liveWorkspaces(workspaces), projects, tasks);
  let n = 0;
  for (const l of lines) {
    if (l.line.state === 'hung-up' || now - Date.parse(l.line.lastActivityAt) > RECENT_MS) continue;
    if (!lineWorkspace(l, ix)) n++;
  }
  return n;
}

export function nudgeVisible(count: number, dismissedAt: number | undefined): boolean {
  return count > NUDGE_THRESHOLD && count > (dismissedAt ?? 0) + NUDGE_THRESHOLD;
}

export interface Names {
  folders: { id: string; name: string }[];
  workspaces: { id: string; name: string }[];
  lines: LocalLine[];
  tasks: Task[];
}

export function describe(a: Action, all: Action[], n: Names): { key: `org.kind.${ActionKind}`; params: Record<string, string | number> } {
  const line = (id?: string) => {
    const l = n.lines.find((x) => x.line.id === id);
    return l?.local?.title || l?.line.branch || id || '';
  };
  const ws = (id?: string) => n.workspaces.find((w) => w.id === id)?.name ?? id ?? '';
  const folder = (f?: string) => all.find((x) => x.kind === 'create_folder' && x.ref === f)?.name ?? n.folders.find((x) => x.id === f)?.name ?? f ?? '';
  const base = (p?: string) => p?.replace(/\/+$/, '').split('/').pop() || p || '';
  const name = (() => {
    if (a.label) return a.label;
    switch (a.kind) {
      case 'create_folder':
      case 'create_group':
        return a.name ?? '';
      case 'create_workspace':
        return base(a.path);
      case 'move_workspace':
      case 'archive_workspace':
        return ws(a.workspaceId);
      case 'set_task_status':
        return n.tasks.find((x) => x.id === a.taskId)?.title ?? a.taskId ?? '';
      default:
        return line(a.lineId);
    }
  })();
  return { key: `org.kind.${a.kind}`, params: { name, folder: folder(a.folder), n: a.lines?.length ?? 0, status: a.status ?? '' } };
}

export function tally(results: Result[]): Record<Result['status'], number> {
  const out = { done: 0, skipped: 0, failed: 0 };
  for (const r of results) out[r.status]++;
  return out;
}

const enc = encodeURIComponent;

export const organizeApi = (c: DaemonClient) => ({
  deterministic: () => c.request<Proposal>('POST', '/v1/organize/deterministic', {}),
  proposals: () => c.request<Proposal[]>('GET', '/v1/organize/proposals'),
  get: (id: string) => c.request<Proposal>('GET', `/v1/organize/proposals/${enc(id)}`),
  apply: (id: string, actionIds: string[]) => c.request<{ proposalId: string; results: Result[] }>('POST', `/v1/organize/proposals/${enc(id)}/apply`, { actionIds }),
  dismiss: (id: string) => c.request<{ id: string; status: string }>('DELETE', `/v1/organize/proposals/${enc(id)}`),
  start: (body: { agent: string; model?: string; effort?: string }) => c.request<{ task: Task; mcp: 'configured' | 'manual' }>('POST', '/v1/organize/start', body),
});
