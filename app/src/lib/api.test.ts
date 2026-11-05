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
});
