import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setClient, type DaemonClient } from '../daemon/client';
import { itemsOf } from './host-watch';
import { restoreHosts } from './host-ops';
import { MockDaemon } from '../daemon/mock';
import { settings } from '../settings/settings.svelte';
import { app } from '../state.svelte';
import type { ConnState, DaemonEvent, HostTarget } from '../types';
import { EMPTY_LAYOUT, workspace } from '../workspace/workspace.svelte';
import { quietMock } from '../../test/quiet-mock';

class Daemon extends MockDaemon {
  connects: HostTarget[] = [];
  disconnects = 0;
  subCount = 0;
  fail: string | null = null;
  hold: Promise<void> | null = null;

  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    this.connects.push(target);
    if (this.hold) await this.hold;
    if (this.fail) {
      onState({ phase: 'error', target, error: this.fail });
      throw new Error(this.fail);
    }
    onState({ phase: 'connected', target });
  }

  override async disconnect(): Promise<void> {
    this.disconnects++;
    await super.disconnect();
  }

  override subscribe(onEvent: (e: DaemonEvent) => void): () => void {
    this.subCount++;
    const off = super.subscribe(onEvent);
    return () => {
      this.subCount--;
      off();
    };
  }
}

const made = vi.hoisted(() => [] as unknown[]);
vi.mock('./host-client', () => {
  return {
    newHostClient: vi.fn(async (): Promise<DaemonClient> => {
      const { MockDaemon: M } = await import('../daemon/mock');
      const d = new (class extends M {
        connects: HostTarget[] = [];
        disconnects = 0;
        override async connect(target: HostTarget, onState: (s: ConnState) => void) {
          this.connects.push(target);
          onState({ phase: 'connected', target });
        }
        override async disconnect() {
          this.disconnects++;
          await super.disconnect();
        }
      })({ live: false, delayMs: 0 });
      made.push(d);
      return d;
    }),
  };
});

type Made = MockDaemon & { connects: HostTarget[]; disconnects: number };
const local: HostTarget = { kind: 'local', label: 'Local' };
const prod: HostTarget = { kind: 'ssh', host: 'prod', label: 'prod' };
const gpu: HostTarget = { kind: 'ssh', host: 'gpu', label: 'gpu' };
const snapshot = JSON.parse(JSON.stringify(settings)) as Record<string, unknown>;

let m: Daemon;

beforeEach(async () => {
  made.length = 0;
  m = new Daemon({ live: false, delayMs: 0 });
  setClient(m);
  app.client = m;
  await app.connect(local);
});

afterEach(async () => {
  for (const l of [...app.others]) await app.removeHost(l.key);
  await app.disconnect();
  workspace.layout = EMPTY_LAYOUT;
  const s = settings as unknown as Record<string, unknown>;
  for (const k of Object.keys(s)) if (!(k in snapshot)) delete s[k];
  Object.assign(s, structuredClone(snapshot));
});

describe('one window per host (the default)', () => {
  it('replaces the host of the window and opens no second connection', async () => {
    await app.connect(prod);
    expect(app.hosts).toEqual([]);
    expect(app.link).toBeNull();
    expect(app.others).toEqual([]);
    expect(made).toEqual([]);
    expect(m.connects).toEqual([local, prod]);
  });
});

describe('several hosts in one window', () => {
  it('adds a host as the active one and keeps the previous host connected in the background', async () => {
    await app.connect(prod, 'add');
    expect(app.conn.target).toEqual(prod);
    expect(app.connected).toBe(true);
    expect(app.hosts.map((l) => l.key)).toEqual(['local', 'ssh:prod']);
    expect(app.others.map((l) => l.key)).toEqual(['local']);
    expect(app.others[0].watching).toBe(true);
    expect(m.disconnects).toBe(0);
    expect((made[0] as Made).connects).toEqual([prod]);
    expect(app.client).toBe(made[0]);
  });

  it('switches back without reconnecting, and follows the other host in the background', async () => {
    await app.connect(prod, 'add');
    await app.switchHost('local');
    expect(app.conn.target).toEqual(local);
    expect(app.client).toBe(m);
    expect(m.connects).toEqual([local]);
    expect(app.others.map((l) => l.key)).toEqual(['ssh:prod']);
    expect(app.others[0].watching).toBe(true);
    expect(app.hosts.map((l) => l.key)).toEqual(['local', 'ssh:prod']);
    expect(m.subCount).toBe(1);
  });

  it('lists the sessions of a background host from its own daemon, apart from the main area', async () => {
    await app.connect(prod, 'add');
    await app.switchHost('local');
    const other = app.others[0];
    await vi.waitFor(() => expect(itemsOf(other).items.length).toBeGreaterThan(0));
    const before = app.data;
    (made[0] as MockDaemon).emit('line.updated', { ...other.data.lines[0], line: { ...other.data.lines[0].line, title: 'renamed on prod' } });
    expect(app.data).toBe(before);
  });

  it('switches to a host already in the window instead of connecting it again', async () => {
    await app.connect(prod, 'add');
    await app.connect(local, 'add');
    expect(app.conn.target).toEqual(local);
    expect(made).toHaveLength(1);
    expect(m.connects).toEqual([local]);
  });

  it('adds a host in the background without touching the main area', async () => {
    const before = app.client;
    await app.addHost(gpu, false);
    await vi.waitFor(() => expect(app.others[0]?.watching).toBe(true));
    expect(app.client).toBe(before);
    expect(app.conn.target).toEqual(local);
    expect(app.hosts.map((l) => l.key)).toEqual(['local', 'ssh:gpu']);
  });

  it('removing a background host disconnects only that host', async () => {
    await app.connect(prod, 'add');
    await app.removeHost('local');
    expect(m.disconnects).toBe(1);
    expect(app.conn.target).toEqual(prod);
    expect(app.hosts.map((l) => l.key)).toEqual(['ssh:prod']);
    expect((made[0] as Made).disconnects).toBe(0);
  });

  it('disconnecting the active host brings another host of the window into the main area', async () => {
    await app.connect(prod, 'add');
    await app.disconnect();
    expect((made[0] as Made).disconnects).toBe(1);
    expect(app.conn.target).toEqual(local);
    expect(app.connected).toBe(true);
    expect(app.others).toEqual([]);
    expect(settings.explicitlyDisconnected).not.toBe(true);
  });

  it('moves a host to a window of its own, then lets go of it here', async () => {
    const open = vi.fn();
    vi.stubGlobal('open', open);
    vi.stubGlobal('location', { href: 'http://app.test/?theme=dark', search: '?theme=dark' });
    await app.connect(prod, 'add');
    await app.moveToWindow('ssh:prod');
    const url = new URL(open.mock.calls[0][0]);
    expect(JSON.parse(url.searchParams.get('connect')!)).toEqual(prod);
    expect(app.hosts.map((l) => l.key)).toEqual(['local']);
    expect((made[0] as Made).disconnects).toBe(1);
  });

  it('does not move the only host of a window', async () => {
    const open = vi.fn();
    vi.stubGlobal('open', open);
    await app.moveToWindow('local');
    expect(open).not.toHaveBeenCalled();
  });

  it('remembers the hosts of the main window for the next launch', async () => {
    await app.connect(prod, 'add');
    expect(settings.windowHosts).toEqual([local, prod]);
    await app.removeHost('ssh:prod');
    expect(settings.windowHosts).toEqual([]);
  });

  it('drops a host that failed to connect when another one is added', async () => {
    m.fail = 'refused';
    await app.connect(local);
    m.fail = null;
    await app.connect(prod, 'add');
    expect(app.conn.target).toEqual(prod);
    expect(app.connected).toBe(true);
    expect(app.others).toEqual([]);
    expect(app.hosts).toEqual([]);
  });

  it('brings a host that is still connecting into the main area once it is ready', async () => {
    await app.addHost(gpu, false);
    const other = app.others[0];
    other.ready = false;
    await app.switchHost('ssh:gpu');
    expect(app.connected).toBe(false);
    other.onState({ phase: 'connected', target: gpu });
    await vi.waitFor(() => expect(app.connected).toBe(true));
    expect(app.conn.target).toEqual(gpu);
  });
});

describe('extra hosts', () => {
  it('get the port settings before they connect, so they never auto-forward with the defaults', async () => {
    settings.portsAutoForward = false;
    const order: string[] = [];
    const { newHostClient } = await import('./host-client');
    vi.mocked(newHostClient).mockImplementationOnce(async () => {
      const d = quietMock();
      d.configurePorts = async (s) => void order.push(`ports ${s.autoForward}`);
      d.connect = async (target, onState) => {
        order.push('connect');
        onState({ phase: 'connected', target });
      };
      return d;
    });
    await app.addHost(gpu, false);
    expect(order).toEqual(['ports false', 'connect']);
  });

  it('tells the user when a background host needs an answer, and brings it forward with its question', async () => {
    let ask: ((s: ConnState) => void) | null = null;
    const { newHostClient } = await import('./host-client');
    vi.mocked(newHostClient).mockImplementationOnce(async () => {
      const d = quietMock();
      d.connect = (target, onState) => {
        ask = onState;
        return new Promise(() => {});
      };
      return d;
    });
    const open = vi.spyOn(app, 'open');
    await app.addHost(gpu, false);
    ask!({ phase: 'connecting', target: gpu, prompt: { id: 1, kind: 'password', text: 'Password:' } as never });
    expect(app.toast?.text).toBe('gpu needs an answer to connect');
    app.runToastAction();
    await vi.waitFor(() => expect(app.conn.target).toEqual(gpu));
    expect(open).toHaveBeenCalledWith('connect');
    app.overlay = null;
  });
});

describe('launch', () => {
  it('reconnects the other hosts of the main window in the background', async () => {
    await app.disconnect();
    Object.assign(settings, { lastHost: local, explicitlyDisconnected: false, windowHosts: [local, prod, gpu, { kind: 'direct', host: 'h1', label: 'phone' }], hostReconnect: true });
    await app.init();
    await vi.waitFor(() => expect(app.others.map((l) => l.key)).toEqual(['ssh:prod', 'ssh:gpu']));
    expect(app.conn.target).toEqual(local);
  });

  it('drops the extra host connections a reloaded page left behind', async () => {
    await app.disconnect();
    const drop = vi.fn(async () => {});
    (m as DaemonClient).dropSlots = drop;
    await app.init();
    expect(drop).toHaveBeenCalledTimes(1);
  });

  it('keeps them closed when reconnecting at launch is off', async () => {
    await app.disconnect();
    Object.assign(settings, { lastHost: local, explicitlyDisconnected: false, windowHosts: [local, prod], hostReconnect: false });
    await app.init();
    expect(app.others).toEqual([]);
    expect(made).toEqual([]);
  });
});

describe('restoreHosts', () => {
  const local: HostTarget = { kind: 'local', label: 'Local' };
  const p: HostTarget = { kind: 'ssh', host: 'prod', label: 'prod' };

  it('brings back the other hosts once, without the first one or phone hosts', () => {
    expect(restoreHosts([local, p, p, { kind: 'direct', host: 'h1', label: 'x' }], local)).toEqual([p]);
    expect(restoreHosts(undefined, local)).toEqual([]);
  });
});
