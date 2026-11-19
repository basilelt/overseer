import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { HostLink } from '../../host/host-link.svelte';
import { itemsOf, load, watch } from '../../host/host-watch';
import { MockDaemon } from '../../daemon/mock';
import { settings } from '../../settings/settings.svelte';
import { app } from '../../state.svelte';
import HostSections from './HostSections.svelte';

const html = (filter = '') => render(HostSections, { props: { filter, showArchived: false } }).body;

describe('HostSections', () => {
  const before = { link: app.link, others: app.others, conn: app.conn, hostsFolded: settings.hostsFolded };
  let local: HostLink;
  let prod: HostLink;

  beforeEach(async () => {
    local = new HostLink({ kind: 'local', label: 'Local' }, new MockDaemon({ live: false, delayMs: 0 }));
    prod = new HostLink({ kind: 'ssh', host: 'prod', label: 'prod' }, new MockDaemon({ live: false, delayMs: 0 }));
    await prod.connect();
    watch(prod);
    await load(prod);
    app.link = local;
    app.conn = { phase: 'connected', target: local.target };
    app.others = [prod];
    settings.hostsFolded = [];
  });

  afterEach(async () => {
    await prod.close();
    Object.assign(app, { link: before.link, others: before.others, conn: before.conn });
    settings.hostsFolded = before.hostsFolded;
  });

  it('shows one section per host, the active one with the usual view', () => {
    const out = html();
    expect(out).toContain('aria-label="Local"');
    expect(out).toContain('aria-label="prod"');
    expect(out).toMatch(/class="spaces[ "]/);
    expect(out.match(/aria-current="true"/g)).toHaveLength(1);
    expect(out).toContain('This computer');
  });

  it('lists the sessions of a background host under its own section', () => {
    const name = itemsOf(prod).items.find((i) => i.kind !== 'shell')?.title;
    expect(name).toBe('PROJ-10844 docker images multi-arch');
    expect(html()).toContain(name!);
  });

  it('lists the sessions of a background workspace in its saved tab order', () => {
    const items = itemsOf(prod).items;
    const keys = ['line:ln_01', 'line:ln_02', 'line:ln_08', 'line:ln_12'].filter((k) => items.find((i) => i.key === k)!.state !== 'closed');
    expect(keys.length).toBeGreaterThan(2);
    const titles = (order: string[]) => order.map((k) => items.find((i) => i.key === k)!.title);
    const at = (tabs: string[]) => {
      prod.ws = { ...prod.ws, workspaces: prod.ws.workspaces.map((w) => (w.id === 'ws_webshop' ? { ...w, tabs } : w)) };
      const full = html();
      const out = full.slice(full.indexOf('aria-label="prod"'));
      return titles(keys)
        .map((n) => [n, out.indexOf(n)] as const)
        .sort((a, b) => a[1] - b[1])
        .map((x) => x[0]);
    };
    const reversed = keys.slice().reverse();
    expect(at(reversed)).toEqual(titles(reversed));
    expect(at(keys)).toEqual(titles(keys));
  });

  it('folds a host section to its header', () => {
    settings.hostsFolded = ['ssh:prod', 'local'];
    const out = html();
    expect(out).not.toMatch(/class="spaces[ "]/);
    expect(out).not.toContain(itemsOf(prod).items[0].title);
    expect(out).toContain('Expand prod');
  });

  it('frames each open host section as one box around its workspaces', () => {
    settings.hostsFolded = ['ssh:prod'];
    const out = html();
    const local = out.match(/<section class="host[^"]*"[^>]*aria-label="Local"/)![0];
    const prod = out.match(/<section class="host[^"]*"[^>]*aria-label="prod"/)![0];
    expect(local).toMatch(/\bopen\b/);
    expect(prod).not.toMatch(/\bopen\b/);
    expect(out.indexOf('aria-label="Local"')).toBeLessThan(out.search(/class="spaces[ "]/));
  });

  it('shows the state of a host that is not connected instead of its sessions', () => {
    prod.conn = { phase: 'reconnecting', target: prod.target, attempt: 2 };
    expect(html()).toMatch(/class="meta mono svelte-[\w-]+ warn"[^>]*>[^<]*2/);
  });

  it('shows a short reconnecting state that fits the header, with the full state in a tooltip', () => {
    prod.conn = { phase: 'reconnecting', target: prod.target, attempt: 3 };
    expect(html()).toMatch(/class="meta mono svelte-[\w-]+ warn" title="Reconnecting \(attempt 3\)">Reconnecting \(3\)</);
  });

  it('offers to add a host to the window', () => {
    expect(html()).toContain('Add a host to this window');
  });
});
