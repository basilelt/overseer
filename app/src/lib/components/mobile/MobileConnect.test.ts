import { afterEach, describe, expect, it, vi } from 'vitest';
import mobileSource from './MobileConnect.svelte?raw';
import desktopSource from '../overlays/ConnectDialog.svelte?raw';

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
  return render(C, { props: { onclose: () => {} } }).body.replace(/<!--.*?-->/g, '');
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

describe('connect screen actions', () => {
  it('enters the demo from the host list and from a failed connection on the phone', () => {
    expect(mobileSource.match(/onclick=\{\(\) => startDemo\(app\.client\)\}/g)).toHaveLength(2);
    expect(mobileSource).toMatch(/onclick=\{\(\) => exitDemo\(\)\}/);
  });

  it('enters the demo from the host list and from a failed connection on the desktop', () => {
    expect(desktopSource).toMatch(/onclick=\{\(\) => \(demo \? exitDemo\(\) : startDemo\(app\.client\)\)\}/);
    expect(desktopSource).toMatch(/\{#if canDemo\}<button class="btn" onclick=\{\(\) => startDemo\(app\.client\)\}>/);
  });
});
