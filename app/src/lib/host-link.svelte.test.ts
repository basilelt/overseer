import { afterEach, describe, expect, it, vi } from 'vitest';
import { DaemonError } from './client';
import { HostLink } from './host-link.svelte';
import { MockDaemon } from './mock';
import type { ConnState, HostTarget } from './types';

const prod: HostTarget = { kind: 'ssh', host: 'prod', label: 'prod' };

class Daemon extends MockDaemon {
  states: ConnState[] = [];
  override async connect(target: HostTarget, onState: (s: ConnState) => void) {
    onState({ phase: 'connecting', target });
    onState({ phase: 'connected', target });
  }
}

let link: HostLink | null = null;
afterEach(async () => {
  await link?.close();
  link = null;
});

const make = async () => {
  const d = new Daemon({ live: false, delayMs: 0 });
  link = new HostLink(prod, d);
  await link.connect();
  return { d, link };
};

describe('HostLink', () => {
  it('is keyed by its host and ready once connected', async () => {
    const { link } = await make();
    expect(link.key).toBe('ssh:prod');
    expect(link.ready).toBe(true);
    expect(link.conn.phase).toBe('connected');
  });

  it('passes connection changes to the app while active', async () => {
    const { link } = await make();
    const seen: string[] = [];
    link.listener = (s, prev) => seen.push(`${prev.phase}>${s.phase}`);
    link.onState({ phase: 'reconnecting', target: prod, attempt: 1 });
    link.onState({ phase: 'connected', target: prod });
    expect(seen).toEqual(['connected>reconnecting', 'reconnecting>connected']);
  });

  it('raises a question asked in the background, once per prompt, and not while active', async () => {
    const { link } = await make();
    const asked = vi.fn();
    link.onPrompt = asked;
    const prompt = { id: 1, kind: 'password', text: 'Password:' } as never;
    link.onState({ phase: 'connecting', target: prod, prompt });
    link.onState({ phase: 'connecting', target: prod, prompt });
    expect(asked).toHaveBeenCalledTimes(1);
    link.listener = () => {};
    link.onState({ phase: 'connecting', target: prod, prompt: { id: 2, kind: 'password', text: 'Password:' } as never });
    expect(asked).toHaveBeenCalledTimes(1);
  });

  it('lists nothing and listens to nothing until watched', async () => {
    const { d, link } = await make();
    const sub = vi.spyOn(d, 'subscribe');
    expect(link.items).toEqual([]);
    expect(sub).not.toHaveBeenCalled();
  });

  it('loads its sessions and workspaces when watched, then follows its events', async () => {
    const { d, link } = await make();
    link.watch();
    await vi.waitFor(() => expect(link.items.length).toBeGreaterThan(0));
    expect(link.ws.workspaces.length).toBeGreaterThan(0);
    const first = link.data.lines[0];
    const changes: string[] = [];
    link.onChange = (_l, _p, next) => changes.push(next.lines.find((l) => l.line.id === first.line.id)?.local?.title ?? '');
    d.emit('line.updated', { ...first, local: { ...first.local, title: 'renamed' } });
    expect(changes).toEqual(['renamed']);
  });

  it('starts from the snapshot the app showed, without its activity', async () => {
    const { link } = await make();
    link.ready = false;
    link.watch({ data: { lines: [], terms: [], tasks: [], activity: { a: {} as never } }, ws: { workspaces: [], folders: [], groups: [] }, projects: [{ path: '/p', name: 'p' } as never] });
    expect(link.data.activity).toEqual({});
    expect(link.projects).toHaveLength(1);
  });

  it('reloads after a reconnect while watched', async () => {
    const { d, link } = await make();
    link.watch();
    await vi.waitFor(() => expect(link.items.length).toBeGreaterThan(0));
    const lines = vi.spyOn(d, 'request');
    link.onState({ phase: 'reconnecting', target: prod, attempt: 1 });
    link.onState({ phase: 'connected', target: prod });
    await vi.waitFor(() => expect(lines.mock.calls.some(([, p]) => String(p).startsWith('/v1/lines'))).toBe(true));
  });

  it('keeps what it had when a load fails, and treats a daemon without workspaces as none', async () => {
    const { d, link } = await make();
    link.watch();
    await vi.waitFor(() => expect(link.items.length).toBeGreaterThan(0));
    const before = link.data.lines;
    vi.spyOn(d, 'request').mockImplementation(async (_m, path) => {
      if (String(path).startsWith('/v1/workspaces')) throw new DaemonError(404, 'nope');
      throw new DaemonError(500, 'down');
    });
    await link.load();
    expect(link.data.lines).toBe(before);
    expect(link.ws.workspaces).toEqual([]);
  });

  it('applies workspace and project events to its own lists', async () => {
    const { d, link } = await make();
    link.watch();
    await vi.waitFor(() => expect(link.ws.workspaces.length).toBeGreaterThan(0));
    const before = link.projects.length;
    d.emit('project.added', { project: { path: '/new', name: 'new' } });
    expect(link.projects.length).toBe(before + 1);
    d.emit('project.added', { project: { path: '/new', name: 'new' } });
    expect(link.projects.length).toBe(before + 1);
  });

  it('stops listening and disconnects when closed', async () => {
    const { d, link } = await make();
    link.watch();
    const off = vi.spyOn(d, 'disconnect');
    await link.close();
    expect(link.watching).toBe(false);
    expect(link.ready).toBe(false);
    expect(link.conn.phase).toBe('idle');
    expect(off).toHaveBeenCalled();
  });
});
