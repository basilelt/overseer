import { afterEach, describe, expect, it, vi } from 'vitest';
import { DaemonError } from './client';
import { HostLink } from './host-link.svelte';
import { itemsOf, load, watch } from './host-watch';
import { MockDaemon } from './mock';
import type { ConnState, HostTarget } from './types';

const prod: HostTarget = { kind: 'ssh', host: 'prod', label: 'prod' };

class Daemon extends MockDaemon {
  override async connect(target: HostTarget, onState: (s: ConnState) => void) {
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

describe('host-watch', () => {
  it('lists nothing and listens to nothing until watched', async () => {
    const { d, link } = await make();
    const sub = vi.spyOn(d, 'subscribe');
    expect(itemsOf(link).items).toEqual([]);
    expect(sub).not.toHaveBeenCalled();
  });

  it('loads its sessions and workspaces when watched, then follows its events', async () => {
    const { d, link } = await make();
    watch(link);
    watch(link);
    await vi.waitFor(() => expect(itemsOf(link).items.length).toBeGreaterThan(0));
    expect(link.ws.workspaces.length).toBeGreaterThan(0);
    const first = link.data.lines[0];
    const changes: string[] = [];
    link.onChange = (_l, _p, next) => changes.push(next.lines.find((l) => l.line.id === first.line.id)?.local?.title ?? '');
    d.emit('line.updated', { ...first, local: { ...first.local, title: 'renamed' } });
    expect(changes).toEqual(['renamed']);
    expect(itemsOf(link).itemWs).toBeInstanceOf(Map);
  });

  it('starts from the snapshot the app showed, without its activity', async () => {
    const { link } = await make();
    link.ready = false;
    watch(link, { data: { lines: [], terms: [], tasks: [], activity: { a: {} as never } }, ws: { workspaces: [], folders: [], groups: [] }, projects: [{ path: '/p', name: 'p' } as never] });
    expect(link.data.activity).toEqual({});
    expect(link.projects).toHaveLength(1);
  });

  it('reloads after a reconnect while watched', async () => {
    const { d, link } = await make();
    watch(link);
    await vi.waitFor(() => expect(itemsOf(link).items.length).toBeGreaterThan(0));
    const req = vi.spyOn(d, 'request');
    link.onState({ phase: 'reconnecting', target: prod, attempt: 1 });
    link.onState({ phase: 'connected', target: prod });
    await vi.waitFor(() => expect(req.mock.calls.some(([, p]) => String(p).startsWith('/v1/lines'))).toBe(true));
  });

  it('keeps what it had when a load fails, and treats a daemon without workspaces as none', async () => {
    const { d, link } = await make();
    watch(link);
    await vi.waitFor(() => expect(itemsOf(link).items.length).toBeGreaterThan(0));
    const before = link.data.lines;
    vi.spyOn(d, 'request').mockImplementation(async (_m, path) => {
      if (String(path).startsWith('/v1/workspaces')) throw new DaemonError(404, 'nope');
      throw new DaemonError(500, 'down');
    });
    await load(link);
    expect(link.data.lines).toBe(before);
    expect(link.ws.workspaces).toEqual([]);
  });

  it('applies workspace and project events to its own lists, once', async () => {
    const { d, link } = await make();
    watch(link);
    await vi.waitFor(() => expect(link.ws.workspaces.length).toBeGreaterThan(0));
    const before = link.projects.length;
    d.emit('project.added', { project: { path: '/new', name: 'new' } });
    d.emit('project.added', { project: { path: '/new', name: 'new' } });
    expect(link.projects.length).toBe(before + 1);
  });

  it('stops following once unwatched', async () => {
    const { d, link } = await make();
    watch(link);
    await vi.waitFor(() => expect(itemsOf(link).items.length).toBeGreaterThan(0));
    link.unwatch();
    const before = link.data;
    d.emit('line.updated', { ...before.lines[0], local: { ...before.lines[0].local, title: 'later' } });
    expect(link.data).toBe(before);
    expect(itemsOf(link).items).toEqual([]);
  });
});
