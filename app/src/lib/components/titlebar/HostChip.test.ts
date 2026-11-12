import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { HostLink } from '../../host-link.svelte';
import { MockDaemon } from '../../mock';
import { app } from '../../state.svelte';
import HostChip from './HostChip.svelte';
import src from './HostChip.svelte?raw';

describe('HostChip', () => {
  const before = { link: app.link, others: app.others, conn: app.conn };
  afterEach(() => Object.assign(app, before));

  it('names the host of a one-host window', () => {
    const local = new HostLink({ kind: 'local', label: 'Local' }, new MockDaemon({ live: false }));
    Object.assign(app, { link: local, others: [], conn: { phase: 'connected', target: local.target } });
    expect(render(HostChip).body).toMatch(/class="label[^"]*">Local</);
  });

  it('counts the hosts of a shared window, with a dot each', () => {
    const local = new HostLink({ kind: 'local', label: 'Local' }, new MockDaemon({ live: false }));
    const a = new HostLink({ kind: 'ssh', host: 'a', label: 'a' }, new MockDaemon({ live: false }));
    const b = new HostLink({ kind: 'ssh', host: 'b', label: 'b' }, new MockDaemon({ live: false }));
    Object.assign(app, { link: local, others: [a, b], conn: { phase: 'connected', target: local.target } });
    const out = render(HostChip).body;
    expect(out).toMatch(/class="label[^"]*">3 hosts</);
    expect(out.match(/class="dot[ "]/g)?.length).toBe(3);
  });

  it('picks a host the way the connect dialog would', () => {
    expect(src).toContain('connectHow(settings, h, { active: hostKey(conn.target), keys: [...here], live: app.connected })');
    expect(src).toContain("app.open('connect', `ask:${JSON.stringify(h)}`)");
    expect(src).toContain('app.switchHost(l.key)');
  });
});
