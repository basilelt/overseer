import { describe, expect, it } from 'vitest';
import { DaemonError } from './client';
import { MockDaemon } from './mock';

const make = (switchboard: boolean) => new MockDaemon({ live: false, delayMs: 0, switchboard });

describe('mock /v1/import/switchboard', () => {
  it('reports nothing found by default', async () => {
    const s = await make(false).request<{ found: boolean; dir: string | null; settings: object }>('GET', '/v1/import/switchboard');
    expect(s).toMatchObject({ found: false, dir: null, imported: false, settings: {} });
  });

  it('answers the contract shape when found', async () => {
    const s = await make(true).request<Record<string, unknown>>('GET', '/v1/import/switchboard');
    expect(Object.keys(s).sort()).toEqual(['dir', 'found', 'hooks', 'imported', 'running', 'sessions', 'settings', 'tasks', 'workspaces']);
  });

  it('imports and then reports imported and stopped', async () => {
    const m = make(true);
    const r = await m.request<Record<string, unknown>>('POST', '/v1/import/switchboard', {});
    expect(Object.keys(r).sort()).toEqual(['hooks', 'imported', 'sessions', 'stopped', 'tasks', 'workspaces']);
    expect(r).toMatchObject({ imported: true, stopped: true });
    expect(await m.request('GET', '/v1/import/switchboard')).toMatchObject({ imported: true, running: false });
  });

  it('returns 404 with the daemon message when nothing is found', async () => {
    const err = await make(false).request('POST', '/v1/import/switchboard', {}).catch((e) => e);
    expect(err).toBeInstanceOf(DaemonError);
    expect(err).toMatchObject({ status: 404, message: 'no switchboard data found' });
  });

  it('rejects other methods', async () => {
    await expect(make(true).request('DELETE', '/v1/import/switchboard')).rejects.toMatchObject({ status: 404 });
  });
});
