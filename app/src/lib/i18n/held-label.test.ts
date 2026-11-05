import { describe, expect, it } from 'vitest';
import { en } from './en';
import { dict as fr } from './fr';

const STOPPED_KEYS = ['group.stopped', 'wsstate.stopped', 'session.state.stopped'] as const;
const PAUSED_KEYS = ['group.paused', 'wsstate.paused', 'session.state.paused'] as const;

describe('label of a session without a live process', () => {
  it('reads Stopped in English when the process is gone, never Held', () => {
    for (const k of STOPPED_KEYS) expect(en[k]).toBe('Stopped');
    expect(en['chat.held']).toBe('Session stopped');
  });

  it('reads Fermée in French when the process is gone, never a waiting state', () => {
    for (const k of STOPPED_KEYS) expect(fr[k]).toBe('Fermée');
    expect(fr['chat.held']).toBe('Session fermée');
  });

  it('reads Paused in English when the user paused it on purpose', () => {
    for (const k of PAUSED_KEYS) expect(en[k]).toBe('Paused');
    expect(en['chat.paused']).toBe('Session paused');
  });

  it('reads En pause in French when the user paused it on purpose', () => {
    for (const k of PAUSED_KEYS) expect(fr[k]).toBe('En pause');
    expect(fr['chat.paused']).toBe('Session en pause');
  });

  it('reads Ready, not Idle, for a live session that waits for the next prompt', () => {
    for (const k of ['group.ready', 'wsstate.ready', 'session.state.ready'] as const) expect([en[k], fr[k]]).toEqual(['Ready', 'Prête']);
  });
});
