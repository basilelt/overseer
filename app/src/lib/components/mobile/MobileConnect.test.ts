import { afterEach, describe, expect, it, vi } from 'vitest';

const store = new Map<string, string>();

async function screen(file: 'mobile' | 'desktop', opts: { demo?: boolean; client?: 'tauri' | 'mock' }) {
  store.clear();
  if (opts.demo) store.set('overseer.demo', '1');
  vi.stubGlobal('localStorage', { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v), removeItem: (k: string) => void store.delete(k) });
  vi.resetModules();
  const { render } = await import('svelte/server');
  const { app } = await import('../../state.svelte');
  app.client = opts.client ? ({ kind: opts.client } as never) : null;
  const C = file === 'mobile' ? (await import('./MobileConnect.svelte')).default : (await import('../overlays/ConnectDialog.svelte')).default;
  let html = render(C, { props: { onclose: () => {} } }).body;
  for (let prev = ''; prev !== html; ) {
    prev = html;
    html = html.replace(/<!--[\s\S]*?--!?>/g, '');
  }
  return html;
}

afterEach(() => vi.unstubAllGlobals());

describe.each(['mobile', 'desktop'] as const)('the %s connect screen', (file) => {
  it('offers Try the demo in the native app', async () => {
    const html = await screen(file, { client: 'tauri' });
    expect(html).toContain('Try the demo');
    expect(html).toContain('No server yet?');
    expect(html).not.toContain('Exit demo');
  });

  it('does not offer the demo where the app already runs on sample data', async () => {
    const html = await screen(file, { client: 'mock' });
    expect(html).not.toContain('Try the demo');
    expect(html).not.toContain('Exit demo');
  });

  it('offers Exit demo instead while in the demo', async () => {
    const html = await screen(file, { demo: true, client: 'mock' });
    expect(html).toContain('Exit demo');
    expect(html).not.toContain('Try the demo');
  });
});
