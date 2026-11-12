import { afterEach, describe, expect, it, vi } from 'vitest';
import { HostLink } from './host-link.svelte';
import { MockDaemon } from './mock';
import type { ConnState, HostTarget } from './types';

const prod: HostTarget = { kind: 'ssh', host: 'prod', label: 'prod' };

class Daemon extends MockDaemon {
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

  it('reloads after a reconnect only while watched', async () => {
    const { link } = await make();
    const reload = vi.fn();
    link.reload = reload;
    link.onState({ phase: 'reconnecting', target: prod, attempt: 1 });
    link.onState({ phase: 'connected', target: prod });
    expect(reload).not.toHaveBeenCalled();
    link.watching = true;
    link.onState({ phase: 'reconnecting', target: prod, attempt: 1 });
    link.onState({ phase: 'connected', target: prod });
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('stops its event stream and disconnects when closed', async () => {
    const { d, link } = await make();
    const off = vi.fn();
    Object.assign(link, { off, watching: true });
    const disc = vi.spyOn(d, 'disconnect');
    await link.close();
    expect(off).toHaveBeenCalled();
    expect(link.watching).toBe(false);
    expect(link.ready).toBe(false);
    expect(link.conn.phase).toBe('idle');
    expect(disc).toHaveBeenCalled();
  });
});
