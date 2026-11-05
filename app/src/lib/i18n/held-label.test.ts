import { describe, expect, it } from 'vitest';
import { en } from './en';
import { dict as fr } from './fr';

const STATE_KEYS = ['group.held', 'chat.status.held'] as const;

describe('label of a session without a live process', () => {
  it('reads Stopped in English, never Held', () => {
    for (const k of STATE_KEYS) expect(en[k]).toBe('Stopped');
    expect(en['chat.held']).toBe('Session stopped');
  });

  it('reads Fermée in French, never a waiting state', () => {
    for (const k of STATE_KEYS) expect(fr[k]).toBe('Fermée');
    expect(fr['chat.held']).toBe('Session fermée');
  });
});
