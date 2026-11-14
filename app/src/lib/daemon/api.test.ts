import { describe, expect, it } from 'vitest';
import { api } from './api';
import type { DaemonClient } from './client';

const recorder = () => {
  const sent: string[] = [];
  const c = { request: async (method: string, path: string) => void sent.push(`${method} ${path}`) } as unknown as DaemonClient;
  return { sent, a: api(c) };
};

describe('api.close', () => {
  it('ends the session and its term by default', async () => {
    const { sent, a } = recorder();
    await a.close('ln 1');
    expect(sent).toEqual(['POST /v1/lines/ln%201/close?dryRun=false']);
  });

  it('asks the daemon to leave a shell in the term when shell is set', async () => {
    const { sent, a } = recorder();
    await a.close('ln 1', true);
    expect(sent).toEqual(['POST /v1/lines/ln%201/close?dryRun=false&shell=true']);
  });

  it('sends force when the user closes a session that is waiting for them anyway', async () => {
    const { sent, a } = recorder();
    await a.close('ln 1', false, true);
    expect(sent).toEqual(['POST /v1/lines/ln%201/close?dryRun=false&force=true']);
  });
});

describe('api.uninstall', () => {
  it('asks for a dry run by default', async () => {
    const calls: unknown[][] = [];
    const c = { request: async (...a: unknown[]) => void calls.push(a) } as unknown as DaemonClient;
    await api(c).uninstall();
    expect(calls).toEqual([['POST', '/v1/uninstall', { dryRun: true }]]);
  });

  it('sends dryRun false only when applying', async () => {
    const calls: unknown[][] = [];
    const c = { request: async (...a: unknown[]) => void calls.push(a) } as unknown as DaemonClient;
    await api(c).uninstall(false);
    expect(calls).toEqual([['POST', '/v1/uninstall', { dryRun: false }]]);
  });
});
