import { center } from './notifications.svelte';
import type { EntryInput } from './notifications';
import { isMuted, kindOf, MAX_COOLDOWN_S, route, toggleMute, type NotifyKind } from './notify-prefs';
import { saveSettings, settings } from '../settings/settings.svelte';
import type { app as App, ToastAction } from '../state.svelte';
import { hostKey } from '../window';

type AppLike = typeof App;

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

const cooled = cooldownGate();

export const hasFocus = () => typeof document !== 'undefined' && document.hasFocus();

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

export interface OsNote {
  kind: NotifyKind;
  title: string;
  body?: string;
  target?: string;
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
      import('./os-send')
        .then((m) => m.sendOs(app, { kind, title: entry.title, body: [entry.source, entry.body].filter(Boolean).join(' · '), target }))
        .then((ok) => ok || fallback())
        .catch(fallback);
  }
  return true;
}
