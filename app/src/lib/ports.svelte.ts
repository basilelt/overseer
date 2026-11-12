import { untrack } from 'svelte';
import { isTauri } from './client';
import { t } from './i18n.svelte';
import { closedForwards, EMPTY_PORTS, forwardNotices, forwardUrl, portChip, portCount, portRows, portSettings } from './ports';
import { settings } from './settings.svelte';
import type { app as App } from './state.svelte';
import type { Forward, PortsState } from './types';

class PortsStore {
  state = $state.raw<PortsState>(EMPTY_PORTS);
  rows = $derived(portRows(this.state));
  count = $derived(portCount(this.state));
  chip = $derived(portChip(this.state));
  flash = $state(0);
}

export const ports = new PortsStore();

export async function openExternal(url: string) {
  if (isTauri()) {
    const { nativeBrowser } = await import('./browser');
    await nativeBrowser.external(url);
  } else window.open(url, '_blank', 'noopener');
}

export function announce(app: Pick<typeof App, 'notice' | 'openUrl'>, host: string, added: Forward[], closed: Forward[]) {
  for (const f of added) {
    const url = forwardUrl(f);
    app.notice(
      { kind: 'portForwarded', title: t('ports.forwarded', { port: f.remotePort }), body: `${f.remotePort} → ${url}`, source: f.process ?? f.project, action: { kind: 'url', url }, coalesce: `port:${host}:${f.remotePort}`, quiet: true },
      { label: t('ports.openBrowser'), run: () => app.openUrl(url, { termId: f.termId }) },
    );
  }
  for (const f of closed) {
    app.notice({ kind: 'portClosed', title: t('ports.closed', { port: f.remotePort }), body: undefined, action: undefined, source: f.process ?? f.project, coalesce: `port:${host}:${f.remotePort}`, quiet: true });
  }
  if (added.length) ports.flash++;
}

export function startPorts(app: typeof App): () => void {
  return $effect.root(() => {
    const session = $derived(app.connected && app.conn.phase === 'connected' && app.client ? `${app.conn.target.kind}:${app.conn.target.host ?? ''}` : null);
    const config = $derived(JSON.stringify(portSettings(settings)));

    $effect(() => {
      if (!session) return;
      const host = session;
      let prev: PortsState | null = null;
      return untrack(() => app.client)!.watchPorts((s) => {
        ports.state = s;
        announce(app, host, forwardNotices(s), prev ? closedForwards(prev, s) : []);
        prev = s;
      });
    });

    $effect(() => {
      if (!session) return;
      untrack(() => app.client)
        ?.configurePorts(JSON.parse(config))
        .catch(() => {});
    });

    $effect(() => {
      if (!app.connected) ports.state = EMPTY_PORTS;
    });

    // The other hosts of the window follow the same port settings.
    $effect(() => {
      const c = JSON.parse(config);
      for (const l of app.others) untrack(() => l.client.configurePorts(c).catch(() => {}));
    });
  });
}
