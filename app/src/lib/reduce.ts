import type { Activity, DaemonEvent, LocalLine, Task, Term } from './types';

export interface Snapshot {
  lines: LocalLine[];
  terms: Term[];
  tasks: Task[];
  activity: Record<string, Activity>;
}

function upsert<T>(list: T[], item: T, key: (t: T) => string): T[] {
  const id = key(item);
  const i = list.findIndex((x) => key(x) === id);
  if (i < 0) return [...list, item];
  const next = list.slice();
  next[i] = item;
  return next;
}

export function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) {
    const y = b as unknown[];
    return a.length === y.length && a.every((v, i) => deepEqual(v, y[i]));
  }
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  for (const k in x) if (x[k] !== undefined && !deepEqual(x[k], y[k])) return false;
  for (const k in y) if (y[k] !== undefined && x[k] === undefined) return false;
  return true;
}

const same = deepEqual;

const unwrap = <T>(data: any, key: string): T => (data && typeof data === 'object' && key in data ? data[key] : data);

export type Touched = Map<string, number>;

export function touchedKey(e: DaemonEvent): string | null {
  const d = e.data;
  if (/^line\.(created|updated|state_changed|hung_up)$/.test(e.type)) return d?.line?.id ? `line:${d.line.id}` : null;
  if (/^term\.(created|updated|exited)$/.test(e.type)) {
    const id = unwrap<Term>(d, 'term')?.id;
    return id ? `term:${id}` : null;
  }
  if (/^task\.(created|updated)$/.test(e.type)) {
    const id = unwrap<Task>(d, 'task')?.id;
    return id ? `task:${id}` : null;
  }
  if (e.type === 'task.deleted') {
    const id = d?.id ?? d?.task?.id;
    return id ? `task:${id}` : null;
  }
  return null;
}

export function mergeFetched<T>(fetched: T[], current: T[], kind: string, key: (t: T) => string, touched: Touched, since: number): T[] {
  const fresh = (id: string) => (touched.get(`${kind}:${id}`) ?? 0) > since;
  if (![...touched].some(([k, n]) => n > since && k.startsWith(`${kind}:`))) return fetched;
  const now = new Map(current.map((x) => [key(x), x]));
  const out: T[] = [];
  const seen = new Set<string>();
  for (const x of fetched) {
    const id = key(x);
    seen.add(id);
    if (!fresh(id)) out.push(x);
    else if (now.has(id)) out.push(now.get(id)!);
  }
  for (const [id, x] of now) if (fresh(id) && !seen.has(id)) out.push(x);
  return out;
}

export function applyEvent(s: Snapshot, e: DaemonEvent): Snapshot {
  switch (e.type) {
    case 'line.created':
    case 'line.updated':
    case 'line.state_changed':
    case 'line.hung_up': {
      const line = e.data?.line;
      if (!line?.id) return s;
      const prev = s.lines.find((l) => l.line.id === line.id);
      const local = e.data.local ? { ...prev?.local, ...e.data.local } : prev?.local;
      const next = { line: { ...prev?.line, ...line }, local };
      if (prev && same(prev, next)) return s;
      return { ...s, lines: upsert(s.lines, next, (l) => l.line.id) };
    }
    case 'line.activity': {
      const { lineId, activity } = e.data ?? {};
      if (!lineId || !activity) return s;
      const i = s.lines.findIndex((l) => l.line.id === lineId && l.local?.needsInput !== activity.needsInput);
      if (i < 0 && same(s.activity[lineId], activity)) return s;
      let lines = s.lines;
      if (i >= 0) {
        lines = s.lines.slice();
        lines[i] = { ...lines[i], local: { ...lines[i].local, needsInput: activity.needsInput } };
      }
      return { ...s, lines, activity: { ...s.activity, [lineId]: activity } };
    }
    case 'term.created':
    case 'term.updated':
    case 'term.exited': {
      const term = unwrap<Term>(e.data, 'term');
      if (!term?.id) return s;
      const prev = s.terms.find((t) => t.id === term.id);
      const next = { ...prev, ...term };
      if (prev && same(prev, next)) return s;
      return { ...s, terms: upsert(s.terms, next, (t) => t.id) };
    }
    case 'task.created':
    case 'task.updated': {
      const task = unwrap<Task>(e.data, 'task');
      if (!task?.id) return s;
      const prev = s.tasks.find((t) => t.id === task.id);
      if (prev && same(prev, task)) return s;
      return { ...s, tasks: upsert(s.tasks, task, (t) => t.id) };
    }
    case 'task.deleted': {
      const id = e.data?.id ?? e.data?.task?.id;
      return id ? { ...s, tasks: s.tasks.filter((t) => t.id !== id) } : s;
    }
    default:
      return s;
  }
}
