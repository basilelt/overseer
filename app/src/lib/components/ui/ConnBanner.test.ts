import { afterEach, describe, expect, it, vi } from 'vitest';

const store = new Map<string, string>();

async function rendered(demo: boolean) {
  store.clear();
  if (demo) store.set('overseer.demo', '1');
  vi.stubGlobal('localStorage', { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) });
  vi.resetModules();
  const { app } = await import('../../state.svelte');
  app.conn = { phase: 'reconnecting', attempt: 2, target: { kind: 'local', label: 'Local' } } as typeof app.conn;
  const { render } = await import('svelte/server');
  const { default: ConnBanner } = await import('./ConnBanner.svelte');
  return render(ConnBanner, { props: {} }).body;
}

afterEach(() => vi.unstubAllGlobals());

describe('ConnBanner', () => {
  it('shows the reconnecting banner for a real server', async () => {
    expect(await rendered(false)).toContain('banner');
  });

  it('shows no server banner in the demo', async () => {
    expect(await rendered(true)).not.toContain('banner');
  });
});
