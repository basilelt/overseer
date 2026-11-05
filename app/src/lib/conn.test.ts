import { describe, expect, it } from 'vitest';
import { ATTACH_STABLE_MS, ATTACH_TRIES, attachRetryMs, attemptAfterDetach } from './conn';

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
