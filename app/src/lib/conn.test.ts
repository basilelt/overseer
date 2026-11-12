import { describe, expect, it } from 'vitest';
import { ATTACH_STABLE_MS, ATTACH_TRIES, attachRetryMs, attemptAfterDetach, backoffMs, steps } from './conn';
import type { ConnState } from './types';

describe('terminal re-attach', () => {
  const handshake = new Error('websocket handshake failed: HTTP error: 400 Bad Request');

  it('retries a failed handshake with growing, capped delays', () => {
    expect(attachRetryMs(0, false, handshake, () => 1)).toBe(500);
    expect(attachRetryMs(3, false, handshake, () => 1)).toBe(4000);
    expect(attachRetryMs(ATTACH_TRIES - 1, false, 'tunnel closed', () => 1)).toBe(16000);
  });

  it('gives up after a bounded number of tries', () => {
    expect(attachRetryMs(ATTACH_TRIES, false, handshake)).toBeNull();
    expect(attachRetryMs(ATTACH_TRIES, false)).toBeNull();
  });

  it('does not retry an exited or unknown terminal', () => {
    expect(attachRetryMs(0, true)).toBeNull();
    expect(attachRetryMs(0, false, new Error('websocket handshake failed: HTTP error: 404 Not Found'))).toBeNull();
  });

  it('reconnects a dropped stream quickly and resets the budget once it stayed up', () => {
    expect(attachRetryMs(0, false, undefined, () => 1)).toBe(500);
    expect(attemptAfterDetach(4, ATTACH_STABLE_MS)).toBe(0);
    expect(attemptAfterDetach(4, 200)).toBe(4);
  });
});

describe('conn steps', () => {
  const target = { kind: 'ssh' as const, host: 'h', label: 'h' };
  const st = (phase: ConnState['phase']): ConnState => ({ phase, target });
  const status = (s: ReturnType<typeof steps>) => s.map((x) => x.status);

  it('marks progress while connecting', () => {
    expect(status(steps(st('checking'), ['connecting', 'checking']))).toEqual(['done', 'active', 'pending', 'pending', 'pending']);
    expect(status(steps(st('starting'), ['connecting', 'checking', 'starting']))).toEqual(['done', 'done', 'skipped', 'active', 'pending']);
  });

  it('skips install when the server was up to date', () => {
    expect(status(steps(st('connected'), ['connecting', 'checking', 'starting', 'connected']))).toEqual(['done', 'done', 'skipped', 'done', 'done']);
    expect(status(steps(st('connected'), ['installing', 'connected']))).toEqual(['done', 'done', 'done', 'done', 'done']);
  });

  it('fails the furthest step on error', () => {
    expect(status(steps(st('error'), ['connecting', 'checking', 'installing', 'error']))).toEqual(['done', 'done', 'failed', 'pending', 'pending']);
    expect(status(steps(st('error'), ['error']))).toEqual(['failed', 'pending', 'pending', 'pending', 'pending']);
  });

  it('shows the server download only when the phone downloads it', () => {
    expect(steps(st('downloading'), ['connecting', 'checking', 'downloading']).map((s) => s.id)).toEqual(['connecting', 'checking', 'downloading', 'installing', 'starting', 'connected']);
    expect(status(steps(st('installing'), ['connecting', 'checking', 'downloading', 'installing']))).toEqual(['done', 'done', 'done', 'active', 'pending', 'pending']);
    expect(steps(st('connected'), ['checking', 'starting', 'connected']).map((s) => s.id)).not.toContain('downloading');
  });

  it('shows the last reached step as active while reconnecting', () => {
    expect(status(steps(st('reconnecting'), ['connecting', 'reconnecting']))).toEqual(['active', 'pending', 'pending', 'pending', 'pending']);
  });

  it('backs off exponentially with jitter and a cap', () => {
    expect(backoffMs(0, () => 0)).toBe(250);
    expect(backoffMs(0, () => 1)).toBe(500);
    expect(backoffMs(3, () => 1)).toBe(4000);
    expect(backoffMs(20, () => 1)).toBe(30000);
    expect(backoffMs(20, () => 0)).toBe(15000);
  });
});
