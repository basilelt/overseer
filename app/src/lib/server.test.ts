import { describe, expect, it } from 'vitest';
import { serverBanner } from './server';
import type { ConnState } from './types';

const target = { kind: 'ssh' as const, host: 'workstation', label: 'workstation' };
const conn = (s: Partial<ConnState>): ConnState => ({ phase: 'connected', target, version: '0.7.0', ...s });
const update = (terms: number) => ({ running: '0.7.0', installed: '0.8.0', terms, reason: 'hot upgrade: not found' });

describe('server update banner', () => {
  it('stays hidden without a pending update or while not connected', () => {
    expect(serverBanner(conn({}))).toBeNull();
    expect(serverBanner(conn({ phase: 'reconnecting', server: update(2) }))).toBeNull();
    expect(serverBanner(conn({ phase: 'error', server: update(2) }))).toBeNull();
  });

  it('counts the terminals that a restart closes', () => {
    expect(serverBanner(conn({ server: update(3) }))).toEqual({
      key: 'server.update',
      id: 'ssh:workstation:0.7.0:0.8.0',
      version: '0.8.0',
      running: '0.7.0',
      n: 3,
      reason: 'hot upgrade: not found',
    });
    expect(serverBanner(conn({ server: update(1) }))?.key).toBe('server.updateOne');
    expect(serverBanner(conn({ server: update(0) }))?.key).toBe('server.updateIdle');
  });

  it('stays dismissed for the same host and versions only', () => {
    expect(serverBanner(conn({ server: update(2) }), 'ssh:workstation:0.7.0:0.8.0')).toBeNull();
    expect(serverBanner(conn({ server: { ...update(2), installed: '0.8.1' } }), 'ssh:workstation:0.7.0:0.8.0')?.version).toBe('0.8.1');
    expect(serverBanner(conn({ target: { kind: 'local', label: 'Local' }, server: update(2) }), 'ssh:workstation:0.7.0:0.8.0')?.id).toBe('local::0.7.0:0.8.0');
  });
});
