import type { EntryKind } from './notifications';

export type Delivery = 'os' | 'app' | 'off';
export const NOTIFY_KINDS = ['needsInput', 'finished', 'failed', 'allIdle', 'ports', 'updates', 'uploads', 'errors'] as const;
export type NotifyKind = (typeof NOTIFY_KINDS)[number];

export interface QuietHours {
  on: boolean;
  start: string;
  end: string;
  days: number[];
}

export interface NotifyPrefs {
  notifyKinds: Record<NotifyKind, Delivery>;
  notifyQuiet: QuietHours;
  notifyOnlyUnfocused: boolean;
  notifySkipWatched: boolean;
  notifyMuted: Record<string, string[]>;
}

export const DEFAULT_KINDS: Record<NotifyKind, Delivery> = {
  needsInput: 'os',
  finished: 'os',
  failed: 'os',
  allIdle: 'os',
  ports: 'app',
  updates: 'app',
  uploads: 'app',
  errors: 'app',
};

export const DEFAULT_QUIET: QuietHours = { on: false, start: '22:00', end: '08:00', days: [0, 1, 2, 3, 4, 5, 6] };
export const MAX_COOLDOWN_S = 120;

const KIND_OF: Record<EntryKind, NotifyKind> = {
  needsInput: 'needsInput',
  taskTesting: 'finished',
  finished: 'finished',
  failed: 'failed',
  allIdle: 'allIdle',
  portForwarded: 'ports',
  portClosed: 'ports',
  update: 'updates',
  upload: 'uploads',
  error: 'errors',
};

export function kindOf(k: EntryKind): NotifyKind {
  return KIND_OF[k] ?? 'errors';
}

const DELIVERIES: Delivery[] = ['os', 'app', 'off'];

export function migrateKinds(raw: Record<string, unknown>): Record<NotifyKind, Delivery> {
  const out = { ...DEFAULT_KINDS };
  const legacy: [string, NotifyKind[]][] = [
    ['notifyNeedsInput', ['needsInput']],
    ['notifyFinished', ['finished']],
    ['notifyFailed', ['failed']],
  ];
  for (const [k, kinds] of legacy) if (raw[k] === false) for (const kind of kinds) out[kind] = 'off';
  const stored = raw.notifyKinds as Record<string, unknown> | undefined;
  if (stored && typeof stored === 'object') for (const k of NOTIFY_KINDS) if (DELIVERIES.includes(stored[k] as Delivery)) out[k] = stored[k] as Delivery;
  return out;
}

const minutes = (hhmm: string): number => {
  const m = /^(\d{1,2}):(\d{2})$/.exec(hhmm);
  return m ? (Number(m[1]) % 24) * 60 + (Number(m[2]) % 60) : 0;
};

export function inQuietHours(q: QuietHours, d: Date): boolean {
  if (!q.on || !q.days.length) return false;
  const now = d.getHours() * 60 + d.getMinutes();
  const day = d.getDay();
  const s = minutes(q.start);
  const e = minutes(q.end);
  if (s === e) return q.days.includes(day);
  if (s < e) return q.days.includes(day) && now >= s && now < e;
  return (q.days.includes(day) && now >= s) || (q.days.includes((day + 6) % 7) && now < e);
}

export type Route = 'os' | 'app' | 'silent' | 'off';

export interface RouteCtx {
  focused: boolean;
  watching: boolean;
  muted: boolean;
  now: Date;
}

export function route(kind: NotifyKind, p: NotifyPrefs, c: RouteCtx): Route {
  const d = p.notifyKinds[kind] ?? DEFAULT_KINDS[kind];
  if (d === 'off') return 'off';
  if (c.muted || (c.watching && p.notifySkipWatched)) return 'silent';
  if (d === 'app' || (c.focused && p.notifyOnlyUnfocused) || inQuietHours(p.notifyQuiet, c.now)) return 'app';
  return 'os';
}

export interface Membership {
  workspaceOf(key: string): string | undefined;
  groupsOf(key: string): string[];
}

export function isMuted(muted: string[] | undefined, target: string | undefined, m: Membership): boolean {
  if (!muted?.length || !target) return false;
  if (muted.includes(target)) return true;
  const ws = m.workspaceOf(target);
  return (!!ws && muted.includes(`w:${ws}`)) || m.groupsOf(target).some((g) => muted.includes(`g:${g}`));
}

export function toggleMute(all: Record<string, string[]>, host: string, target: string): Record<string, string[]> {
  const list = all[host] ?? [];
  const next = list.includes(target) ? list.filter((x) => x !== target) : [...list, target];
  const out = { ...all, [host]: next };
  if (!next.length) delete out[host];
  return out;
}

export type Channel = 'agents' | 'ports' | 'updates' | 'errors' | 'quiet';

export function channelFor(kind: NotifyKind, sound: boolean): Channel {
  if (!sound) return 'quiet';
  if (kind === 'ports') return 'ports';
  if (kind === 'updates' || kind === 'uploads') return 'updates';
  if (kind === 'errors') return 'errors';
  return 'agents';
}

export function soundFor(os: string, on: boolean): string | undefined {
  if (!on) return undefined;
  if (os === 'macos') return 'Ping';
  if (os === 'windows') return 'Default';
  if (os === 'linux') return 'message-new-instant';
  if (os === 'ios') return 'default';
  return undefined;
}

export type Perm = 'granted' | 'denied' | 'prompt' | 'system' | 'unsupported';

export interface PermEnv {
  tauri: boolean;
  os: string;
  granted?: boolean | null;
  web?: 'granted' | 'denied' | 'default';
  asked: boolean;
}

export const DESKTOP_OS = ['macos', 'windows', 'linux'];

export function permOf(e: PermEnv): Perm {
  if (e.tauri) {
    if (DESKTOP_OS.includes(e.os)) return 'system';
    if (e.granted === true) return 'granted';
    return e.granted === false || e.asked ? 'denied' : 'prompt';
  }
  if (!e.web) return 'unsupported';
  return e.web === 'default' ? 'prompt' : e.web;
}

export function mayAsk(p: Perm, fromUser: boolean): boolean {
  return p === 'prompt' || (fromUser && p === 'denied');
}

export function opensSettings(tauri: boolean, os: string): boolean {
  return tauri && (os === 'macos' || os === 'windows' || os === 'android');
}

export function clickFallback(os: string): boolean {
  return os === 'windows' || os === 'linux';
}

export const FOCUS_FALLBACK_MS = 30_000;
