import { describe, expect, it } from 'vitest';
import { FOCUS_REFRESH_MS, ReviewFeed, type FeedClient } from './feed.svelte';
import type { ReviewRequest, ReviewRequests } from './types';

function deferred() {
  const calls: { path: string; resolve: (v: unknown) => void; reject: (e: unknown) => void }[] = [];
  const client: FeedClient = {
    request: <T>(_m: string, path: string) => new Promise<T>((resolve, reject) => calls.push({ path, resolve: resolve as (v: unknown) => void, reject })),
  };
  return { client, calls };
}

const feedOf = (login: string): ReviewRequests => ({ login, prs: [], fetchedAt: 1 });

describe('ReviewFeed', () => {
  it('drops a late response from the previous host after a host switch', async () => {
    const f = new ReviewFeed();
    const { client, calls } = deferred();
    const a = f.load(client, 'ssh:a');
    const b = f.load(client, 'ssh:b');
    expect(calls).toHaveLength(2);
    calls[1].resolve(feedOf('bob'));
    await b;
    calls[0].resolve(feedOf('alice'));
    await a;
    expect(f.data?.login).toBe('bob');
    expect(f.loading).toBe(false);
  });

  it('clears the previous host data at once when the host changes', async () => {
    const f = new ReviewFeed();
    const { client, calls } = deferred();
    const a = f.load(client, 'ssh:a');
    calls[0].resolve(feedOf('alice'));
    await a;
    void f.load(client, 'ssh:b');
    expect(f.data).toBeNull();
    expect(f.loading).toBe(true);
  });

  it('a stale failure does not overwrite fresh data or leave loading stuck', async () => {
    const f = new ReviewFeed();
    const { client, calls } = deferred();
    const a = f.load(client, 'local');
    const b = f.load(client, 'local');
    calls[1].resolve(feedOf('me'));
    await b;
    calls[0].reject(new Error('boom'));
    expect(await a).toBeNull();
    expect(f.error).toBeNull();
    expect(f.data?.login).toBe('me');
    expect(f.loading).toBe(false);
  });

  it('throttles focus refreshes', async () => {
    let now = 1_000_000;
    const f = new ReviewFeed(() => now);
    const { client, calls } = deferred();
    expect(f.due()).toBe(true);
    const a = f.load(client, 'local');
    expect(f.due()).toBe(false);
    calls[0].resolve(feedOf('me'));
    await a;
    now += FOCUS_REFRESH_MS - 1;
    expect(f.due()).toBe(false);
    now += 1;
    expect(f.due()).toBe(true);
  });

  it('reports a failure only when it hides a non-empty list', async () => {
    const f = new ReviewFeed();
    const { client, calls } = deferred();
    const a = f.load(client, 'local');
    calls[0].reject(new Error('down'));
    expect(await a).toBeNull();
    expect(f.error?.message).toBe('down');
    const b = f.load(client, 'local');
    calls[1].resolve({ login: 'me', prs: [{ number: 1 } as ReviewRequest], fetchedAt: 1 });
    await b;
    const c = f.load(client, 'local');
    calls[2].reject(new Error('down'));
    expect((await c)?.message).toBe('down');
    expect(f.data?.prs).toHaveLength(1);
  });
});
