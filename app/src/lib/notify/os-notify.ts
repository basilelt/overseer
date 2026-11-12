import * as CMD from '../daemon/tauri-commands';
import { isTauri } from '../platform';
import { isDemo } from '../daemon/demo';
import { t } from '../i18n.svelte';
import { center } from './notifications.svelte';
import type { EntryInput } from './notifications';
import {
  channelFor,
  clickFallback,
  DESKTOP_OS,
  FOCUS_FALLBACK_MS,
  isMuted,
  kindOf,
  mayAsk,
  MAX_COOLDOWN_S,
  permOf,
  route,
  soundFor,
  toggleMute,
  type Channel,
  type NotifyKind,
  type Perm,
} from './notify-prefs';
import { saveSettings, settings } from '../settings/settings.svelte';
import type { app as App, ToastAction } from '../state.svelte';
import { hostKey } from '../window';
import { readItem, writeItem, KEY_NOTIFY_ASKED } from '../storage';

type Plugin = typeof import('@tauri-apps/plugin-notification');
type AppLike = typeof App;

export interface Tapped {
  actionId?: string;
  notification?: { id?: number; extra?: Record<string, unknown> } | null;
}

export function osId(n: { key: string | null; title: string }): number {
  let h = 0;
  for (const c of n.key ?? n.title) h = (Math.imul(h, 31) + c.charCodeAt(0)) | 0;
  return (h & 0x7fffffff) || 1;
}

export function routeOf(e: Tapped, known: Map<number, string>): string | null {
  if (e.actionId && e.actionId !== 'tap') return null;
  const extra = e.notification?.extra?.key;
  if (typeof extra === 'string' && extra) return extra;
  const id = e.notification?.id;
  return (id !== undefined && known.get(id)) || null;
}

export function cooldownGate(now: () => number = Date.now, max = 500) {
  const last = new Map<string, number>();
  return (slot: string, ms: number): boolean => {
    const t0 = now();
    if (t0 - (last.get(slot) ?? -Infinity) < ms) return false;
    last.delete(slot);
    last.set(slot, t0);
    if (last.size > max) last.delete(last.keys().next().value!);
    return true;
  };
}

const ASKED = KEY_NOTIFY_ASKED;
const sent = new Map<number, string>();
const cooled = cooldownGate();
let taps: Promise<unknown> | null = null;
let channels: Promise<unknown> | null = null;
let pendingFocus: { key: string; at: number } | null = null;

const hasFocus = () => typeof document !== 'undefined' && document.hasFocus();

function asked(): boolean {
  return readItem(ASKED) === '1';
}

function markAsked() {
  writeItem(ASKED, '1');
}

const plugin = (): Promise<Plugin> => import('@tauri-apps/plugin-notification');

export async function permission(os: string): Promise<Perm> {
  if (isDemo()) return permOf({ tauri: false, os, web: undefined, asked: false });
  if (!isTauri()) return permOf({ tauri: false, os, web: typeof Notification === 'undefined' ? undefined : Notification.permission, asked: false });
  if (DESKTOP_OS.includes(os)) return 'system';
  const granted = await plugin()
    .then((m) => m.isPermissionGranted())
    .catch(() => false);
  return permOf({ tauri: true, os, granted, asked: asked() });
}

export async function requestPermission(os: string, fromUser: boolean): Promise<Perm> {
  const p = await permission(os);
  if (!mayAsk(p, fromUser)) return p;
  markAsked();
  if (!isTauri()) {
    await Notification.requestPermission().catch(() => {});
    return permission(os);
  }
  const r = await plugin()
    .then((m) => m.requestPermission())
    .catch(() => 'denied');
  return r === 'granted' ? 'granted' : 'denied';
}

async function focusKey(app: AppLike, key: string) {
  if (isTauri()) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const w = getCurrentWindow();
      await w.unminimize();
      await w.setFocus();
    } catch {}
  } else globalThis.focus?.();
  app.open(null);
  app.select(key);
}

function listen(app: AppLike, mod: Plugin): Promise<unknown> {
  taps ??= mod
    .onAction((e) => {
      const key = routeOf(e as Tapped, sent);
      if (!key) return;
      pendingFocus = null;
      void focusKey(app, key);
    })
    .catch(() => {});
  return taps;
}

function ensureChannels(mod: Plugin): Promise<unknown> {
  const { Importance, Visibility } = mod;
  const defs: [Channel, number, boolean][] = [
    ['agents', Importance.High, true],
    ['ports', Importance.Default, true],
    ['updates', Importance.Default, true],
    ['errors', Importance.High, true],
    ['quiet', Importance.Low, false],
  ];
  channels ??= Promise.all(defs.map(([id, importance, vibration]) => mod.createChannel({ id, name: t(`notify.channel.${id}`), importance, vibration, visibility: Visibility.Private }).catch(() => {})));
  return channels;
}

export interface OsNote {
  kind: NotifyKind;
  title: string;
  body?: string;
  target?: string;
}

export async function sendOs(app: AppLike, n: OsNote, fromUser = false): Promise<boolean> {
  if (isDemo()) return false;
  const body = settings.notifyPreview ? n.body || undefined : undefined;
  if (!isTauri()) {
    if (fromUser) await requestPermission(app.os, true);
    if (typeof Notification === 'undefined' || Notification.permission !== 'granted') return false;
    const note = new Notification(n.title, { body, tag: n.target, silent: !settings.notifySound });
    if (n.target) note.onclick = () => focusKey(app, n.target!);
    return true;
  }
  const os = app.os;
  const p = await requestPermission(os, fromUser);
  if (p !== 'granted' && p !== 'system') return false;
  const mod = await plugin();
  await listen(app, mod);
  if (os === 'android') await ensureChannels(mod);
  const id = osId({ key: n.target ?? null, title: n.title });
  if (n.target) {
    sent.delete(id);
    sent.set(id, n.target);
    if (sent.size > 200) sent.delete(sent.keys().next().value!);
  }
  if (os === 'linux') await mod.removeActive([{ id }]).catch(() => {});
  mod.sendNotification({
    id,
    title: n.title,
    body,
    extra: n.target ? { key: n.target } : undefined,
    group: n.target,
    channelId: os === 'android' ? channelFor(n.kind, settings.notifySound) : undefined,
    sound: soundFor(os, settings.notifySound),
    autoCancel: true,
  });
  if (n.target && clickFallback(os) && !hasFocus()) pendingFocus = { key: n.target, at: Date.now() };
  return true;
}

export function sendTest(app: AppLike): Promise<boolean> {
  return sendOs(app, { kind: 'needsInput', title: t('notify.testTitle'), body: t('notify.testBody') }, true);
}

export async function openSystemSettings(): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke(CMD.openNotificationSettings);
}

function membership(app: AppLike) {
  return {
    workspaceOf: (k: string) => app.itemWs.get(k),
    groupsOf: (k: string) => app.ws.groups.filter((g) => g.members.includes(k)).map((g) => g.id),
  };
}

export const hostOf = (app: AppLike) => hostKey(app.conn.target);

export function mutedList(app: AppLike): string[] {
  return settings.notifyMuted[hostOf(app)] ?? [];
}

export function isTargetMuted(app: AppLike, target: string): boolean {
  return mutedList(app).includes(target);
}

export function toggleTargetMute(app: AppLike, target: string) {
  saveSettings({ notifyMuted: toggleMute(settings.notifyMuted, hostOf(app), target) });
}

export interface NoticeInput extends EntryInput {
  target?: string;
  toast?: boolean;
}

export function dispatch(app: AppLike, input: NoticeInput, show: (text: string, kind: 'info' | 'error', action?: ToastAction) => void, action?: ToastAction): boolean {
  const { target: explicit, toast, ...entry } = input;
  const kind = kindOf(entry.kind);
  const target = explicit ?? (entry.action?.kind === 'line' ? entry.action.key : undefined);
  const focused = hasFocus();
  const r = route(kind, settings, {
    focused,
    watching: focused && !!target && target === app.selectedKey,
    muted: isMuted(mutedList(app), target, membership(app)),
    now: new Date(),
  });
  if (r === 'off') return false;
  const row = { ...entry, coalesce: entry.coalesce ?? target, quiet: entry.quiet || r === 'silent' };
  if (!center.add(row)) return false;
  const fallback = () => {
    if (toast ?? settings.notifyBackgroundToasts) show(entry.title, entry.kind === 'failed' || entry.kind === 'error' ? 'error' : 'info', action);
  };
  if (r === 'app') fallback();
  else if (r === 'os') {
    const gap = Math.min(MAX_COOLDOWN_S, Math.max(0, settings.notifyCooldown)) * 1000;
    if (cooled(`${entry.kind}|${target ?? entry.coalesce ?? entry.title}`, gap))
      sendOs(app, { kind, title: entry.title, body: [entry.source, entry.body].filter(Boolean).join(' · '), target })
        .then((ok) => ok || fallback())
        .catch(fallback);
  }
  return true;
}

export function startOs(app: AppLike): () => void {
  let done = false;
  let stop: (() => void) | null = null;
  if (isTauri() && !isDemo())
    void plugin().then((mod) => {
      const p = listen(app, mod);
      return p.then((l) => {
        const u = l as { unregister?: () => Promise<void> } | undefined;
        if (!u?.unregister) return;
        if (!done) stop = () => void u.unregister!();
        else {
          void u.unregister();
          if (taps === p) taps = null;
        }
      });
    });
  const onFocus = () => {
    const p = pendingFocus;
    pendingFocus = null;
    if (p && Date.now() - p.at < FOCUS_FALLBACK_MS) {
      app.open(null);
      app.select(p.key);
    }
  };
  globalThis.addEventListener?.('focus', onFocus);
  return () => {
    done = true;
    stop?.();
    taps = null;
    globalThis.removeEventListener?.('focus', onFocus);
  };
}
