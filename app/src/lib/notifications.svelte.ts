import { addEntry, loadEntries, markSubjectRead, saveEntries, storageKey, unreadCount, type Entry, type EntryInput } from './notifications';
import { untrack } from 'svelte';
import type { KV } from './notify';
import { t } from './i18n.svelte';
import type { app as App } from './state.svelte';
import { updater, ut } from './updater.svelte';
import { hostKey } from './window';

export class NotificationCenter {
  entries = $state.raw<Entry[]>([]);
  unread = $derived(unreadCount(this.entries));
  private host = '';

  constructor(
    private store: KV | undefined = globalThis.localStorage,
    private now: () => number = () => Date.now(),
  ) {}

  setHost(host: string) {
    if (host === this.host) return;
    this.host = host;
    this.entries = loadEntries(this.store, host);
  }

  sync(key: string | null) {
    if (key === null || key === storageKey(this.host)) this.entries = loadEntries(this.store, this.host);
  }

  private latest(): Entry[] {
    let raw: string | null = null;
    try {
      raw = this.store?.getItem(storageKey(this.host)) ?? null;
    } catch {}
    return raw === null ? this.entries : loadEntries(this.store, this.host);
  }

  add(input: EntryInput): boolean {
    const next = addEntry(this.latest(), input, this.now());
    if (!next) return false;
    this.set(next);
    return true;
  }

  markRead(id: string) {
    const list = this.latest();
    if (list.some((e) => e.id === id && !e.read)) this.set(list.map((e) => (e.id === id ? { ...e, read: true } : e)));
  }

  markSubjectRead(key: string) {
    const next = markSubjectRead(this.latest(), key);
    if (next) this.set(next);
  }

  markAllRead() {
    const list = this.latest();
    if (unreadCount(list)) this.set(list.map((e) => (e.read ? e : { ...e, read: true })));
  }

  clear() {
    this.set([]);
  }

  private set(list: Entry[]) {
    this.entries = list;
    saveEntries(this.store, this.host, list);
  }
}

export const center = new NotificationCenter();

export function startCenter(app: typeof App): () => void {
  const onStorage = (e: Event) => center.sync((e as StorageEvent).key);
  globalThis.addEventListener?.('storage', onStorage);
  const stop = $effect.root(() => {
    $effect(() => center.setHost(hostKey(app.conn.target)));
    $effect(() => {
      const version = updater.phase === 'available' ? updater.info?.version : undefined;
      if (version) untrack(() => app.notice({ id: `update:${version}`, kind: 'update', title: ut('available', { version }), action: { kind: 'update' } }));
    });
    $effect(() => {
      const s = app.conn.server;
      if (s) untrack(() => app.notice({ id: `server:${s.installed}`, kind: 'update', title: t(s.terms === 0 ? 'server.updateIdle' : s.terms === 1 ? 'server.updateOne' : 'server.update', { version: s.installed, n: s.terms }) }));
    });
  });
  return () => {
    globalThis.removeEventListener?.('storage', onStorage);
    stop();
  };
}

export function runEntry(app: typeof App, e: Entry) {
  center.markRead(e.id);
  const a = e.action;
  if (!a) return;
  app.open(null);
  if (a.kind === 'line') {
    const host = a.host;
    if (host && host !== app.link?.key && app.others.some((l) => l.key === host)) void app.switchHost(host).then(() => app.select(a.key));
    else app.select(a.key);
  }
  else if (a.kind === 'url') app.openUrl(a.url);
  else updater.dialog = true;
}
