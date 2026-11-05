import { afterEach, describe, expect, it, vi } from 'vitest';
import { hostKey, isMainWindow, openWindow, windowLabel } from './window';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

describe('window label', () => {
  it('prefers the Tauri window label, then the ?window= param, then main', () => {
    expect(windowLabel('?window=w-3', { metadata: { currentWindow: { label: 'w-2' } } })).toBe('w-2');
    expect(windowLabel('?window=w-3', undefined)).toBe('w-3');
    expect(windowLabel('?theme=dark&window=w-4', {})).toBe('w-4');
    expect(windowLabel('', undefined)).toBe('main');
    expect(windowLabel('?window=', undefined)).toBe('main');
  });
});

describe('host key', () => {
  it('names each host once, whatever its label', () => {
    expect(hostKey({ kind: 'local' })).toBe('local');
    expect(hostKey({ kind: 'ssh', host: 'box' })).toBe('ssh:box');
    expect(hostKey({ kind: 'direct', host: 'h1' })).toBe('direct:h1');
    expect(hostKey({ kind: 'ssh', host: 'box' })).not.toBe(hostKey({ kind: 'direct', host: 'box' }));
  });
});

describe('isMainWindow', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('is true for the first window and false for extra ones', () => {
    vi.stubGlobal('location', { search: '' });
    expect(isMainWindow()).toBe(true);
    vi.stubGlobal('location', { search: '?window=w-2' });
    expect(isMainWindow()).toBe(false);
    vi.stubGlobal('__TAURI_INTERNALS__', { metadata: { currentWindow: { label: 'main' } } });
    expect(isMainWindow()).toBe(true);
  });
});

describe('openWindow', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    invoke.mockReset();
  });

  it('asks the desktop app for a native window', async () => {
    const open = vi.fn();
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
    vi.stubGlobal('open', open);
    invoke.mockResolvedValue(undefined);
    await openWindow();
    expect(invoke).toHaveBeenCalledWith('window_new');
    expect(open).not.toHaveBeenCalled();
  });

  it('opens a new browser tab with a fresh window label each time', async () => {
    const open = vi.fn();
    vi.stubGlobal('window', {});
    vi.stubGlobal('open', open);
    vi.stubGlobal('location', { href: 'https://sb.local/app?theme=dark&window=w-1' });
    await openWindow();
    await openWindow();
    const urls = open.mock.calls.map(([u, target]) => (expect(target).toBe('_blank'), new URL(u as string)));
    expect(urls.map((u) => u.searchParams.get('theme'))).toEqual(['dark', 'dark']);
    const labels = urls.map((u) => u.searchParams.get('window'));
    expect(labels.every((l) => /^w-\d+$/.test(l!))).toBe(true);
    expect(new Set([...labels, 'w-1']).size).toBe(3);
    expect(invoke).not.toHaveBeenCalled();
  });
});
