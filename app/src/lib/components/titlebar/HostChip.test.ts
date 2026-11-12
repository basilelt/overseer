import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { HostLink } from '../../host-link.svelte';
import { MockDaemon } from '../../mock';
import { app } from '../../state.svelte';
import HostChip from './HostChip.svelte';
import chip from './HostChip.svelte?raw';
import menu from './HostMenu.svelte?raw';
import bar from '../TitleBar.svelte?raw';

describe('title bar startup weight', () => {
  it('keeps only the host chip at startup and loads its menu when it opens', () => {
    expect(chip).toMatch(/const menu = preload\(\(\) => import\('\.\/HostMenu\.svelte'\)\);/);
    expect(chip).toMatch(/\{#if open\}\s*\{#await menu\(\) then \{ default: HostMenu \}\}<HostMenu \{tip\} onclose=\{\(\) => \(open = false\)\} \/>\{\/await\}/);
    expect(chip).not.toMatch(/recentHosts|disconnect/);
  });

  it('closes the host menu before switching or disconnecting', () => {
    expect(menu).toMatch(/function pick\(h: HostTarget\) \{\s*onclose\(\);\s*const how = connectHow/);
    expect(menu).toMatch(/async function disconnect\(\) \{\s*onclose\(\);\s*await app\.disconnect\(\);/);
  });

  it('loads the demo pill only in demo mode', () => {
    expect(bar).toMatch(/const demoPill = isDemo\(\) \? \(\) => import\('\.\/ui\/DemoPill\.svelte'\) : null;/);
    expect(bar).toMatch(/\{#if demoPill\}\{#await demoPill\(\) then \{ default: DemoPill \}\}<DemoPill \/>\{\/await\}\{\/if\}/);
  });
});

describe('HostChip hosts', () => {
  const before = { link: app.link, others: app.others, conn: app.conn };
  afterEach(() => Object.assign(app, before));

  it('names the host of a one-host window', () => {
    const local = new HostLink({ kind: 'local', label: 'Local' }, new MockDaemon({ live: false }));
    Object.assign(app, { link: local, others: [], conn: { phase: 'connected', target: local.target } });
    expect(render(HostChip).body).toMatch(/class="label[^"]*">Local</);
  });

  it('counts the hosts of a shared window', () => {
    const local = new HostLink({ kind: 'local', label: 'Local' }, new MockDaemon({ live: false }));
    const a = new HostLink({ kind: 'ssh', host: 'a', label: 'a' }, new MockDaemon({ live: false }));
    const b = new HostLink({ kind: 'ssh', host: 'b', label: 'b' }, new MockDaemon({ live: false }));
    Object.assign(app, { link: local, others: [a, b], conn: { phase: 'connected', target: local.target } });
    const out = render(HostChip).body;
    expect(out).toMatch(/class="label[^"]*">3 hosts</);
  });

  it('picks a host the way the connect dialog would, from its lazy menu', () => {
    expect(menu).toContain('connectHow(settings, h, { active: hostKey(conn.target), keys: [...here], live: app.connected })');
    expect(menu).toContain("app.open('connect', `ask:${JSON.stringify(h)}`)");
    expect(menu).toContain('app.switchHost(l.key)');
  });
});
