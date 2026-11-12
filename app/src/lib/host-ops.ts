import { newHostClient } from './host-client';
import { HostLink } from './host-link.svelte';
import { itemsOf, watch } from './host-watch';
import { folderName, notificationsFor, notifyGate, prefsOf, stillDue, type Gate } from './notify';
import { MAX_COOLDOWN_S } from './notify-prefs';
import type { Snapshot } from './reduce';
import { hostName } from './hosts';
import { t } from './i18n.svelte';
import { portSettings } from './ports';
import { rememberHost, saveSettings, settings } from './settings.svelte';
import type { app as AppState } from './state.svelte';
import type { DaemonEvent, HostTarget, LocalLine } from './types';
import { hostKey, isMainWindow, openWindow } from './window';

type App = typeof AppState;

function register(app: App, link: HostLink) {
  (app.linkOf ??= new WeakMap()).set(link.client, link);
}

/** The window's own connection becomes a link once a second host joins. */
function wrapFirst(app: App) {
  if (app.link || !app.client) return;
  const l = new HostLink(app.conn.target, app.client);
  l.conn = app.conn;
  l.ready = app.connected;
  l.listener = app.onActiveState;
  register(app, l);
  app.link = l;
  app.hostOrder = [l.key];
}

export async function addHost(app: App, target: HostTarget, activate: boolean): Promise<void> {
  const key = hostKey(target);
  if (hostKey(app.conn.target) === key) return;
  if (app.others.some((l) => l.key === key)) return activate ? app.switchHost(key) : undefined;
  wrapFirst(app);
  const link = new HostLink(target, await newHostClient(`h${++app.slots}`));
  register(app, link);
  // Before connecting: an extra host must not auto-forward its ports with the defaults.
  await link.client.configurePorts(portSettings(settings)).catch(() => {});
  if (!app.hostOrder.includes(key)) app.hostOrder = [...app.hostOrder, key];
  if (!activate) {
    app.others = [...app.others, link];
    follow(app, link);
    link.connect({ serverBinDir: settings.serverBinDir || undefined }).then(
      () => {
        if (link !== app.link && app.others.includes(link)) watch(link);
      },
      (err) => {
        if (link.conn.phase !== 'error') link.conn = { phase: 'error', target, error: String((err as Error)?.message ?? err) };
      },
    );
    saveHosts(app);
    return;
  }
  park(app);
  adopt(app, link);
  await app.connect(target);
}

export async function switchHost(app: App, key: string): Promise<void> {
  const next = app.others.find((l) => l.key === key);
  if (!next) return;
  park(app);
  adopt(app, next);
  if (next.ready) {
    rememberHost(next.target, isMainWindow());
    await attach(app);
  } else app.attachOnReady = true;
  if (app.conn.prompt) app.open('connect');
  saveHosts(app);
}

export async function removeHost(app: App, key: string): Promise<void> {
  if (app.link?.key === key && app.others.length) {
    const gone = app.link;
    app.unsubscribe?.();
    app.unsubscribe = null;
    app.cancelWsRetry();
    gone.listener = null;
    const next = app.others[0];
    adopt(app, next);
    if (next.ready) {
      rememberHost(next.target, isMainWindow());
      void attach(app);
    } else app.attachOnReady = true;
    app.hostOrder = app.hostOrder.filter((k) => k !== key);
    saveHosts(app);
    await gone.close();
    return;
  }
  const link = app.others.find((l) => l.key === key);
  if (!link) return;
  app.others = app.others.filter((l) => l !== link);
  app.hostOrder = app.hostOrder.filter((k) => k !== key);
  saveHosts(app);
  await link.close();
}

export async function moveToWindow(app: App, key: string): Promise<void> {
  const link = app.hosts.find((l) => l.key === key);
  if (!link || app.hosts.length < 2) return;
  await openWindow(link.target);
  await app.removeHost(key);
}

function park(app: App) {
  const prev = app.link;
  app.unsubscribe?.();
  app.unsubscribe = null;
  app.cancelWsRetry();
  if (!prev) return;
  prev.listener = null;
  if (!prev.ready && !app.connected) {
    app.hostOrder = app.hostOrder.filter((k) => k !== prev.key);
    void prev.close();
    return;
  }
  app.others = [...app.others, prev];
  follow(app, prev);
  watch(prev, { data: app.data, ws: app.ws, projects: app.projects });
}

function adopt(app: App, link: HostLink) {
  app.resetHost();
  app.following = null;
  app.multi = new Set();
  app.touched.clear();
  app.added.clear();
  for (const t of app.activityTimers.values()) clearTimeout(t);
  app.activityTimers.clear();
  link.unwatch();
  link.onChange = null;
  link.onPrompt = null;
  app.others = app.others.filter((l) => l !== link);
  app.link = link;
  app.client = link.client;
  app.conn = link.conn;
  link.listener = app.onActiveState;
  app.connected = false;
  app.synced = false;
  app.attachOnReady = false;
  if (link.data.lines.length || link.data.terms.length) {
    app.data = { ...link.data, activity: {} };
    app.ws = link.ws;
    app.projects = link.projects;
  }
}

function follow(app: App, link: HostLink) {
  link.onChange = (l, prev, next, e) => {
    app.onOther?.(l, prev, next, e);
  };
  link.onPrompt = () => app.notify(t('hosts.needsAnswer', { host: hostName(link.target) }), 'info', { label: t('hosts.answer'), run: () => void app.switchHost(link.key) });
}

/** Notifications of a background host of the window: same rules, its own settle and cooldown, named after it. */
export function pushHost(app: App, gates: Map<string, Gate>, l: HostLink, prev: Snapshot, next: Snapshot, e: DaemonEvent | undefined) {
  if (prev.lines === next.lines && prev.terms === next.terms && prev.tasks === next.tasks) return;
  let g = gates.get(l.key);
  if (!g) {
    g = notifyGate(
      (n, id) =>
        app.notice({
          id: `${l.key}|${id}`,
          kind: n.kind,
          title: t('hosts.onHost', { host: hostName(l.target), title: n.title }),
          body: n.body || undefined,
          action: n.key ? { kind: 'line', key: n.key, host: l.key } : undefined,
          target: n.target ?? n.key ?? undefined,
        }),
      (n) => stillDue(l.data, n),
      globalThis.localStorage,
      Date.now,
      () => Math.min(MAX_COOLDOWN_S, Math.max(0, settings.notifyCooldown)) * 1000,
    );
    gates.set(l.key, g);
  }
  const at = e?.at || e?.id || String(Date.now());
  const { itemWs } = itemsOf(l);
  const place = (line: LocalLine) => l.ws.workspaces.find((w) => w.id === itemWs.get(`line:${line.line.id}`))?.name || folderName(line);
  for (const n of notificationsFor(prev, next, prefsOf(settings), place)) g.push(n, at);
}


export function saveHosts(app: App) {
  if (!isMainWindow() || app.mobile) return;
  const list = app.hosts.filter((l) => l.target.kind !== 'direct').map((l) => l.target);
  if (list.length < 2 && !settings.windowHosts?.length) return;
  saveSettings({ windowHosts: list.length < 2 ? [] : list });
}

/** Hosts to bring back at launch beside the first one, without duplicates. */
export function restoreHosts(saved: HostTarget[] | undefined, first: HostTarget): HostTarget[] {
  const seen = new Set([hostKey(first)]);
  const out: HostTarget[] = [];
  for (const h of saved ?? []) {
    if (h.kind === 'direct') continue;
    const k = hostKey(h);
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(h);
  }
  return out;
}

/** Brings back, in the background, the other hosts the main window showed. */
export function restore(app: App, saved: HostTarget[], first: HostTarget) {
  for (const h of restoreHosts(saved, first)) void addHost(app, h, false);
}

/** Shows the active host in the main area once it is connected. */
export async function attach(app: App) {
  if (!app.client) return;
  app.connected = true;
  app.unsubscribe?.();
  app.unsubscribe = app.client.subscribe((e) => app.onEvent(e));
  await app.refresh();
}
