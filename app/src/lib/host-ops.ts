import { newHostClient } from './client';
import { HostLink } from './host-link.svelte';
import { hostName } from './hosts';
import { t } from './i18n.svelte';
import { portSettings } from './ports';
import { rememberHost, settings } from './settings.svelte';
import type { app as AppState } from './state.svelte';
import type { HostTarget } from './types';
import { hostKey, isMainWindow, openWindow } from './window';

type App = typeof AppState;

export async function addHost(app: App, target: HostTarget, activate: boolean): Promise<void> {
  const key = hostKey(target);
  if (app.link?.key === key) return;
  if (app.others.some((l) => l.key === key)) return activate ? app.switchHost(key) : undefined;
  const link = new HostLink(target, await newHostClient(`h${++app.slots}`));
  // Before connecting: an extra host must not auto-forward its ports with the defaults.
  await link.client.configurePorts(portSettings(settings)).catch(() => {});
  if (!app.hostOrder.includes(key)) app.hostOrder = [...app.hostOrder, key];
  if (!activate) {
    app.others = [...app.others, link];
    follow(app, link);
    link.connect({ serverBinDir: settings.serverBinDir || undefined }).then(
      () => {
        if (link !== app.link && app.others.includes(link)) link.watch();
      },
      (err) => {
        if (link.conn.phase !== 'error') link.conn = { phase: 'error', target, error: String((err as Error)?.message ?? err) };
      },
    );
    app.saveHosts();
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
    await app.attach();
  } else app.attachOnReady = true;
  if (app.conn.prompt) app.open('connect');
  app.saveHosts();
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
      void app.attach();
    } else app.attachOnReady = true;
    app.hostOrder = app.hostOrder.filter((k) => k !== key);
    app.saveHosts();
    await gone.close();
    return;
  }
  const link = app.others.find((l) => l.key === key);
  if (!link) return;
  app.others = app.others.filter((l) => l !== link);
  app.hostOrder = app.hostOrder.filter((k) => k !== key);
  app.saveHosts();
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
  prev.watch({ data: app.data, ws: app.ws, projects: app.projects });
}

function adopt(app: App, link: HostLink) {
  app.resetHost();
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
    for (const fn of app.otherListeners) fn(l, prev, next, e);
  };
  link.onPrompt = () => app.notify(t('hosts.needsAnswer', { host: hostName(link.target) }), 'info', { label: t('hosts.answer'), run: () => void app.switchHost(link.key) });
}
