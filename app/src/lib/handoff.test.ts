import { afterEach, describe, expect, it, vi } from 'vitest';
import { takeHandoff } from './handoff';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const prod = { kind: 'ssh' as const, host: 'prod', label: 'prod' };

describe('takeHandoff', () => {
  afterEach(() => {
    invoke.mockReset();
  });

  it('reads the host a native window was opened for', async () => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValueOnce(prod);
    expect(await takeHandoff()).toEqual(prod);
    expect(invoke).toHaveBeenCalledWith('window_handoff');
    invoke.mockResolvedValueOnce(null);
    expect(await takeHandoff()).toBeNull();
    invoke.mockRejectedValueOnce(new Error('unknown command'));
    expect(await takeHandoff()).toBeNull();
  });

  it('reads the host of a browser window from its URL and ignores anything else', async () => {
    vi.stubGlobal('window', {});
    expect(await takeHandoff(`?connect=${encodeURIComponent(JSON.stringify(prod))}`)).toEqual(prod);
    expect(await takeHandoff(`?connect=${encodeURIComponent(JSON.stringify({ kind: 'ssh', host: 'box' }))}`)).toEqual({ kind: 'ssh', host: 'box', label: 'box' });
    expect(await takeHandoff(`?connect=${encodeURIComponent(JSON.stringify({ kind: 'local' }))}`)).toEqual({ kind: 'local', label: 'Local' });
    expect(await takeHandoff('?connect=%7Bbad')).toBeNull();
    expect(await takeHandoff(`?connect=${encodeURIComponent(JSON.stringify({ kind: 'ssh' }))}`)).toBeNull();
    expect(await takeHandoff(`?connect=${encodeURIComponent(JSON.stringify({ kind: 'direct', host: 'h1' }))}`)).toBeNull();
    expect(await takeHandoff('')).toBeNull();
  });
});
