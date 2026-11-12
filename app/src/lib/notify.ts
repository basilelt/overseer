import { agentName } from './agents';
import { isTauri } from './client';
import { t } from './i18n.svelte';
import type { Snapshot } from './reduce';
import { settings, type Settings } from './settings.svelte';
import type { app as App } from './state.svelte';
import type { DaemonEvent, LocalLine } from './types';
import { allIdle } from './workspaces';
import { MAX_COOLDOWN_S } from './notify-prefs';
import { startOs } from './os-notify';
import { KEYS } from './storage';
import { isTesting } from './task-status';

export type NoticeKind = 'needsInput' | 'taskTesting' | 'finished' | 'failed' | 'allIdle';

export interface Notice {
  kind: NoticeKind;
  key: string | null;
  title: string;
  body: string;
  target?: string;
}

export interface Prefs {
  notifyNeedsInput: boolean;
  notifyFinished: boolean;
  notifyFailed: boolean;
}

export function prefsOf(s: Pick<Settings, 'notifyKinds'>): Prefs {
  return { notifyNeedsInput: s.notifyKinds.needsInput !== 'off', notifyFinished: s.notifyKinds.finished !== 'off', notifyFailed: s.notifyKinds.failed !== 'off' };
}

function lineTitle(l: LocalLine): string {
  return l.local?.titleOverride || l.local?.title || l.line.ticketKey || l.line.branch || l.line.id;
}

export type PlaceOf = (l: LocalLine) => string | undefined;

export function folderName(l: LocalLine): string | undefined {
  const p = l.line.worktreePath;
  if (!p || p === 'unknown') return undefined;
  return p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || undefined;
}

const gone = (l: LocalLine) => l.line.state === 'held' || l.line.state === 'hung-up';

function describe(l: LocalLine, place: string | undefined, extra?: string): string {
  const title = lineTitle(l);
  return [title !== place ? title : '', place, extra].filter(Boolean).join(' · ');
}

export interface Change<T> {
  item: T;
  before: T | undefined;
}

export function changes<T>(prev: readonly T[], next: readonly T[], id: (x: T) => string): Change<T>[] {
  if (prev === next) return [];
  const out: Change<T>[] = [];
  let i = 0;
  if (prev.length === next.length) {
    for (; i < next.length; i++) {
      const a = prev[i];
      const b = next[i];
      if (a === b) continue;
      if (id(a) !== id(b)) break;
      out.push({ item: b, before: a });
    }
    if (i === next.length) return out;
    out.length = 0;
  }
  const byId = new Map(prev.map((x) => [id(x), x]));
  for (const item of next) {
    const before = byId.get(id(item));
    if (before !== item) out.push({ item, before });
  }
  return out;
}

const lineId = (l: LocalLine) => l.line.id;
const byOwnId = (x: { id: string }) => x.id;

export function notificationsFor(prev: Snapshot, next: Snapshot, prefs: Prefs, placeOf: PlaceOf = folderName): Notice[] {
  const out: Notice[] = [];
  const lines = prefs.notifyNeedsInput || prefs.notifyFinished ? changes(prev.lines, next.lines, lineId) : [];
  if (prefs.notifyNeedsInput) {
    for (const { item: l, before } of lines) {
      if (!l.local?.needsInput || before?.local?.needsInput || gone(l)) continue;
      const agent = agentName([], l.line.agent);
      out.push({ kind: 'needsInput', key: `line:${l.line.id}`, title: t('notify.needsInput', { title: agent }), body: describe(l, placeOf(l), next.activity[l.line.id]?.status) });
    }
  }
  if (prefs.notifyFinished) {
    for (const { item: l, before } of lines) {
      if (!before?.local?.busy || l.local?.busy !== false || l.local?.needsInput || gone(l)) continue;
      const agent = agentName([], l.line.agent);
      const place = placeOf(l);
      const title = place ? t('notify.agentFinished', { agent, place }) : t('notify.agentDone', { agent });
      out.push({ kind: 'finished', key: `line:${l.line.id}`, title, body: lineTitle(l) === place ? '' : lineTitle(l) });
    }
    for (const { item: task, before } of changes(prev.tasks, next.tasks, byOwnId)) {
      if (!isTesting(task.status) || (before ? isTesting(before.status) : false)) continue;
      const key = task.lineId ? `line:${task.lineId}` : null;
      out.push({ kind: 'taskTesting', key, title: t('notify.taskTesting', { n: task.number }), body: task.title, target: key ?? `task:${task.id}` });
    }
  }
  for (const { item: term, before } of changes(prev.terms, next.terms, byOwnId)) {
    if (!term.exited || before?.exited || (!term.lineId && !term.taskId)) continue;
    const ok = (term.exitCode ?? 0) === 0;
    if (ok ? !prefs.notifyFinished : !prefs.notifyFailed) continue;
    const title = term.titleOverride || term.title || term.cmd.join(' ');
    out.push({
      kind: ok ? 'finished' : 'failed',
      key: term.lineId ? `line:${term.lineId}` : `term:${term.id}`,
      title: t(ok ? 'notify.finished' : 'notify.failed', { title }),
      body: ok ? '' : t('term.exited', { code: term.exitCode ?? '?' }),
    });
  }
  return out;
}

export function needsInputCount(s: Snapshot): number {
  return s.lines.filter((l) => l.local?.needsInput && l.line.state !== 'hung-up').length;
}

export function idleTransitions(before: Map<string, boolean>, after: Map<string, boolean>): string[] {
  return [...after].filter(([k, v]) => v && before.get(k) === false).map(([k]) => k);
}

function idleWatch(a: typeof App): Map<string, boolean> {
  const out = new Map<string, boolean>();
  for (const w of a.ws.workspaces) if (w.notifyIdle) out.set(`w:${w.id}`, allIdle(a.membersOf(w.id)));
  for (const g of a.ws.groups) if (g.notifyIdle) out.set(`g:${g.id}`, allIdle(a.groupItems(g.id)));
  return out;
}

export interface KV {
  getItem(k: string): string | null;
  setItem(k: string, v: string): void;
}

const SEEN_KEY = KEYS.notified;
const SEEN_MAX = 500;
const SETTLE_MS = 1500;
const COOLDOWN_MS = 20_000;

export function stillDue(s: Snapshot, n: Notice): boolean {
  if (!n.key?.startsWith('line:') || (n.kind !== 'needsInput' && n.kind !== 'finished')) return true;
  const l = s.lines.find((x) => `line:${x.line.id}` === n.key);
  if (!l || gone(l)) return false;
  return n.kind === 'needsInput' ? !!l.local?.needsInput : !l.local?.busy && !l.local?.needsInput;
}

export interface Gate {
  push(n: Notice, at: string): void;
  stop(): void;
}

export function notifyGate(send: (n: Notice, id: string) => void, due: (n: Notice) => boolean, store: KV | undefined, now: () => number = Date.now, cooldown: () => number = () => COOLDOWN_MS): Gate {
  let seen: string[] = [];
  try {
    const raw = JSON.parse(store?.getItem(SEEN_KEY) ?? '[]');
    if (Array.isArray(raw)) seen = raw.filter((x) => typeof x === 'string').slice(-SEEN_MAX);
  } catch {}
  const known = new Set(seen);
  const last = new Map<string, number>();
  const pending = new Map<string, { n: Notice; id: string; timer: ReturnType<typeof setTimeout> }>();
  const remember = (id: string) => {
    known.add(id);
    seen.push(id);
    if (seen.length > SEEN_MAX) known.delete(seen.shift()!);
    try {
      store?.setItem(SEEN_KEY, JSON.stringify(seen));
    } catch {}
  };
  const fire = (slot: string) => {
    const p = pending.get(slot);
    pending.delete(slot);
    if (!p || known.has(p.id)) return;
    remember(p.id);
    if (!due(p.n) || now() - (last.get(slot) ?? -Infinity) < cooldown()) return;
    last.set(slot, now());
    send(p.n, p.id);
  };
  return {
    push(n, at) {
      const slot = `${n.kind}|${n.key ?? n.title}`;
      const id = `${slot}|${at}`;
      if (known.has(id)) return;
      clearTimeout(pending.get(slot)?.timer);
      pending.set(slot, { n, id, timer: setTimeout(() => fire(slot), SETTLE_MS) });
    },
    stop() {
      for (const p of pending.values()) clearTimeout(p.timer);
      pending.clear();
    },
  };
}

export function placeIn(app: typeof App): PlaceOf {
  return (l) => {
    const id = app.itemWs.get(`line:${l.line.id}`);
    return app.ws.workspaces.find((w) => w.id === id)?.name || folderName(l);
  };
}

const baseTitle = typeof document !== 'undefined' ? document.title : '';

async function setBadge(count: number) {
  if (isTauri()) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().setBadgeCount(count || undefined);
    } catch {}
  }
  if (typeof document !== 'undefined') document.title = count ? `(${count}) ${baseTitle}` : baseTitle;
}

export function startNotifications(app: typeof App): () => void {
  let lastCount = -1;
  let idle = new Map<string, boolean>();
  const untap = startOs(app);
  const placeOf = placeIn(app);
  const gate = notifyGate(
    (n, id) => app.notice({ id, kind: n.kind, title: n.title, body: n.body || undefined, action: n.key ? { kind: 'line', key: n.key } : undefined, target: n.target ?? n.key ?? undefined }),
    (n) => stillDue(app.data, n),
    globalThis.localStorage,
    Date.now,
    () => Math.min(MAX_COOLDOWN_S, Math.max(0, settings.notifyCooldown)) * 1000,
  );
  const total = () => needsInputCount(app.data) + app.others.reduce((n, l) => n + needsInputCount(l.data), 0);
  const badge = () => {
    const count = total();
    if (count !== lastCount) {
      lastCount = count;
      setBadge(count);
    }
  };
  // Background hosts of the window: same rules, their own settle and cooldown, named after their host.
  const gates = new Map<string, Gate>();
  let stopped = false;
  app.onOther = (l, prev, next, e) => {
    badge();
    // Loaded with the hosts code, which a window with a background host has already loaded.
    void import('./host-ops').then((m) => !stopped && m.pushHost(app, gates, l, prev, next, e));
  };
  const unwatch = app.onChange((prev, next, e: DaemonEvent) => {
    if (prev.lines === next.lines && prev.terms === next.terms && prev.tasks === next.tasks) return;
    const at = e?.at || e?.id || String(Date.now());
    const now = idleWatch(app);
    for (const k of idleTransitions(idle, now)) {
      const name = k.startsWith('w:') ? app.ws.workspaces.find((w) => `w:${w.id}` === k)?.name : app.ws.groups.find((g) => `g:${g.id}` === k)?.name;
      gate.push({ kind: 'allIdle', key: null, title: t('notify.allIdle', { name: name ?? '' }), body: '', target: k }, at);
    }
    idle = now;
    badge();
    for (const n of notificationsFor(prev, next, prefsOf(settings), placeOf)) gate.push(n, at);
  });
  return () => {
    unwatch();
    app.onOther = null;
    stopped = true;
    for (const g of gates.values()) g.stop();
    gate.stop();
    untap();
  };
}
