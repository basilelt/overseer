import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { createRawSnippet } from 'svelte';
import { render } from 'svelte/server';
import { HostLink } from '../../host-link.svelte';
import { MockDaemon } from '../../mock';
import { settings } from '../../settings.svelte';
import { app } from '../../state.svelte';
import HostSections from './HostSections.svelte';
import sidebarSrc from './Sidebar.svelte?raw';

const active = createRawSnippet(() => ({ render: () => '<div class="active-view">ACTIVE VIEW</div>' }));
const html = (filter = '') => render(HostSections, { props: { filter, active } }).body;

describe('HostSections', () => {
  const before = { link: app.link, others: app.others, conn: app.conn, hostsFolded: settings.hostsFolded };
  let local: HostLink;
  let prod: HostLink;

  beforeEach(async () => {
    local = new HostLink({ kind: 'local', label: 'Local' }, new MockDaemon({ live: false }));
    prod = new HostLink({ kind: 'ssh', host: 'prod', label: 'prod' }, new MockDaemon({ live: false }));
    await prod.connect();
    prod.watch();
    await new Promise((r) => setTimeout(r, 0));
    await prod.load();
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
    expect(out).toContain('ACTIVE VIEW');
    expect(out.match(/aria-current="true"/g)).toHaveLength(1);
    expect(out).toContain('this machine');
  });

  it('lists the sessions of a background host under its own section', () => {
    const name = prod.items.find((i) => i.kind !== 'shell')?.title;
    expect(name).toBeTruthy();
    expect(html()).toContain(name!);
  });

  it('folds a host section to its header', () => {
    settings.hostsFolded = ['ssh:prod', 'local'];
    const out = html();
    expect(out).not.toContain('ACTIVE VIEW');
    expect(out).not.toContain(prod.items[0].title);
    expect(out).toContain('Unfold prod');
  });

  it('shows the state of a host that is not connected instead of its sessions', () => {
    prod.conn = { phase: 'reconnecting', target: prod.target, attempt: 2 };
    expect(html()).toMatch(/class="meta mono svelte-[\w-]+ warn"[^>]*>[^<]*2/);
  });

  it('offers to add a host to the window', () => {
    expect(html()).toContain('Add a host to this window');
  });

  it('is shown by the sidebar only when the window has several hosts, around the usual view', () => {
    expect(sidebarSrc).toMatch(/\{#if app\.hosts\.length > 1\}\n\s+\{#await hostSections\(\) then \{ default: HostSections \}\}<HostSections \{filter\} active=\{view\} \/>\{\/await\}/);
  });
});
