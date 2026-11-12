import { afterEach, describe, expect, it, vi } from 'vitest';
import { newHostClient } from './host-client';
import { MockDaemon } from './mock';

describe('newHostClient', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('gives the browser and mock builds a mock daemon of their own', async () => {
    const a = await newHostClient('h1');
    const b = await newHostClient('h2');
    expect(a).toBeInstanceOf(MockDaemon);
    expect(a).not.toBe(b);
  });

  it('gives the desktop app a client on the slot of that host', async () => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
    const c = await newHostClient('h3');
    expect(c.kind).toBe('tauri');
    expect((c as unknown as { slot: string }).slot).toBe('h3');
  });
});
