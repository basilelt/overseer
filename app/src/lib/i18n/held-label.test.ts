import { describe, expect, it } from 'vitest';
import { en } from './en';
import { dict as fr } from './fr';

const CLOSED_KEYS = ['group.stopped', 'wsstate.stopped', 'session.state.closed'] as const;
const PAUSED_KEYS = ['group.paused', 'wsstate.paused', 'session.state.paused'] as const;

describe('label of a session without a live process', () => {
  it('reads Closed in English when the process is gone, never Held', () => {
    for (const k of CLOSED_KEYS) expect(en[k]).toBe('Closed');
    expect(en['chat.held']).toBe('Session stopped');
  });

  it('reads Fermée in French when the process is gone, never a waiting state', () => {
    for (const k of CLOSED_KEYS) expect(fr[k]).toBe('Fermée');
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

  it('reads Open, not Idle or Ready, for a live session that waits for the next prompt', () => {
    for (const k of ['group.ready', 'wsstate.ready', 'session.state.open'] as const) expect([en[k], fr[k]]).toEqual(['Open', 'Ouverte']);
  });

  it('names the five states the same way in both languages', () => {
    const keys = ['running', 'attention', 'open', 'paused', 'closed'] as const;
    expect(keys.map((k) => en[`session.state.${k}`])).toEqual(['Running', 'Requires attention', 'Open', 'Paused', 'Closed']);
    expect(keys.map((k) => fr[`session.state.${k}`])).toEqual(['En cours', 'Attention requise', 'Ouverte', 'En pause', 'Fermée']);
  });

  it('labels the queued pause and the sub-agents count', () => {
    expect([en['session.state.pause_pending'], fr['session.state.pause_pending']]).toEqual(['Pause after this turn', 'Pause à la fin du tour']);
    expect([en['session.subagents'], en['session.subagentsOne']]).toEqual(['{n} sub-agents', '1 sub-agent']);
    expect([fr['session.subagents'], fr['session.subagentsOne']]).toEqual(['{n} sous-agents', '1 sous-agent']);
  });

  it('words the ended label and the sub-agents interrupt reason', () => {
    expect([en['session.ended'], fr['session.ended']]).toEqual(['Session ended', 'Session terminée']);
    expect(en['session.why.subagents']).toBe('Sub-agents are running; interrupt them from the session');
    expect(fr['session.why.subagents']).toBe('Des sous-agents tournent ; interrompez-les depuis la session');
  });

  it('calls a hung-up line ended, and its close action ends the session', () => {
    expect([en['chat.closed'], fr['chat.closed']]).toEqual(['Session ended', 'Session terminée']);
    expect([en['session.action.close'], fr['session.action.close']]).toEqual(['End the session', 'Terminer la session']);
  });
});
