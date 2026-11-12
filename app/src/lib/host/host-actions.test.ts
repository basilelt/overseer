import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { hostMenu, openOnHost, toggleFold } from './host-actions';
import { HostLink } from './host-link.svelte';
import { MockDaemon } from '../daemon/mock';
import { settings } from '../settings/settings.svelte';
import { app } from '../state.svelte';

describe('host actions', () => {
  const before = { link: app.link, others: app.others, conn: app.conn, hostsFolded: settings.hostsFolded };
  let local: HostLink;
  let prod: HostLink;

  beforeEach(() => {
    local = new HostLink({ kind: 'local', label: 'Local' }, new MockDaemon({ live: false }));
    prod = new HostLink({ kind: 'ssh', host: 'prod', label: 'prod' }, new MockDaemon({ live: false }));
    prod.conn = { phase: 'connected', target: prod.target };
    app.link = local;
    app.conn = { phase: 'connected', target: local.target };
    app.others = [prod];
    settings.hostsFolded = [];
  });

  afterEach(() => {
    vi.restoreAllMocks();
    Object.assign(app, { link: before.link, others: before.others, conn: before.conn });
    settings.hostsFolded = before.hostsFolded;
  });

  const labels = (key: string) => hostMenu(key).map((i) => i.label);

  it('offers to bring a background host into the main area, fold it, move it out or disconnect it', () => {
    expect(labels('ssh:prod')).toEqual(['Show in the main area', 'Collapse prod', 'Move to its own window', 'Disconnect, sessions keep running']);
  });

  it('does not offer to show the active host again', () => {
    expect(labels('local')).toEqual(['Collapse Local', 'Move to its own window', 'Disconnect, sessions keep running']);
  });

  it('offers a retry for a host that failed', () => {
    prod.conn = { phase: 'error', target: prod.target, error: 'refused' };
    expect(labels('ssh:prod')).toContain('Retry');
  });

  it('runs each entry on the right host', async () => {
    const sw = vi.spyOn(app, 'switchHost').mockResolvedValue();
    const rm = vi.spyOn(app, 'removeHost').mockResolvedValue();
    const mv = vi.spyOn(app, 'moveToWindow').mockResolvedValue();
    const add = vi.spyOn(app, 'addHost').mockResolvedValue();
    prod.conn = { phase: 'error', target: prod.target, error: 'refused' };
    const [show, retry, fold, move, off] = hostMenu('ssh:prod');
    await show.run!();
    await retry.run!();
    fold.run!();
    await move.run!();
    await off.run!();
    expect(sw).toHaveBeenCalledWith('ssh:prod');
    expect(add).toHaveBeenCalledWith(prod.target, false);
    expect(settings.hostsFolded).toEqual(['ssh:prod']);
    expect(mv).toHaveBeenCalledWith('ssh:prod');
    expect(rm).toHaveBeenCalledWith('ssh:prod');
  });

  it('retries the active host by connecting it again in place', () => {
    const c = vi.spyOn(app, 'connect').mockResolvedValue();
    app.conn = { phase: 'error', target: local.target, error: 'x' };
    hostMenu('local').find((i) => i.label === 'Retry')!.run!();
    expect(c).toHaveBeenCalledWith(local.target);
  });

  it('has nothing for a host not in the window', () => {
    expect(hostMenu('ssh:nope')).toEqual([]);
  });

  it('cannot move the only host of a window', () => {
    app.others = [];
    expect(hostMenu('local').find((i) => i.label === 'Move to its own window')?.disabled).toBe(true);
  });

  it('folds and unfolds a section', () => {
    toggleFold('local');
    expect(settings.hostsFolded).toEqual(['local']);
    toggleFold('local');
    expect(settings.hostsFolded).toEqual([]);
  });

  it('brings the host of a clicked session into the main area first', async () => {
    const order: string[] = [];
    vi.spyOn(app, 'switchHost').mockImplementation(async (k) => void order.push(`switch ${k}`));
    await openOnHost('ssh:prod', () => order.push('select'));
    await openOnHost('local', () => order.push('select'));
    expect(order).toEqual(['switch ssh:prod', 'select', 'select']);
  });
});
