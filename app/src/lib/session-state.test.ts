import { describe, expect, it } from 'vitest';
import { SESSION_ACTIONS, sessionState, type SessionAction } from './session-state';
import type { LocalLine } from './types';

const line = (over: Partial<LocalLine['line']> = {}, local: LocalLine['local'] = { pid: 42 }): LocalLine => ({
  line: { id: 'l1', agent: 'claude', state: 'active', lastActivityAt: '2026-11-05T10:00:00Z', ...over },
  local,
});

const enabled = (l: LocalLine, termId?: string | null) =>
  SESSION_ACTIONS.filter((a) => sessionState(l, termId).actions[a].enabled).sort();

const sorted = (a: SessionAction[]) => [...a].sort();

describe('sessionState: which state a session is in', () => {
  it('is running when the agent is alive and busy', () => {
    expect(sessionState(line({}, { pid: 42, busy: true })).state).toBe('running');
  });

  it('is needs_input when the agent waits for an answer, even while busy', () => {
    expect(sessionState(line({}, { pid: 42, busy: true, needsInput: true })).state).toBe('needs_input');
  });

  it('is ready when the agent is alive and not busy', () => {
    expect(sessionState(line({ state: 'idle' }, { pid: 42 })).state).toBe('ready');
  });

  it('is alive with no pid as long as it has a live Overseer terminal', () => {
    expect(sessionState(line({}, { termId: 't1' })).state).toBe('ready');
    expect(sessionState(line({}, {}), 't1').state).toBe('ready');
    expect(sessionState({ line: line().line }, 't1').state).toBe('ready');
  });

  it('is stopped when the line is active but no process or terminal backs it', () => {
    expect(sessionState(line({}, {})).state).toBe('stopped');
    expect(sessionState({ line: line().line }).state).toBe('stopped');
  });

  it('is pause_pending when a pause is queued on a busy agent', () => {
    expect(sessionState(line({}, { pid: 42, busy: true, holdPending: true })).state).toBe('pause_pending');
  });

  it('stays needs_input when a pause is queued, since the user must answer first', () => {
    expect(sessionState(line({}, { pid: 42, needsInput: true, holdPending: true })).state).toBe('needs_input');
  });

  it('is paused when the user held it on purpose', () => {
    expect(sessionState(line({ state: 'held' }, { heldReason: 'user' })).state).toBe('paused');
  });

  it('is stopped when held because its process is gone', () => {
    expect(sessionState(line({ state: 'held' }, { heldReason: 'lost' })).state).toBe('stopped');
  });

  it('is stopped when held without a reason, the safe reading of an old daemon', () => {
    expect(sessionState(line({ state: 'held' }, {})).state).toBe('stopped');
  });

  it('is closed when the line is hung up, whatever its process says', () => {
    expect(sessionState(line({ state: 'hung-up' }, { pid: 42, busy: true })).state).toBe('closed');
  });

  it('returns when the session was paused, for the "paused for N days" label', () => {
    const s = sessionState(line({ state: 'held' }, { heldReason: 'user', heldAt: '2026-11-01T09:00:00Z' }));
    expect(s.since).toBe('2026-11-01T09:00:00Z');
    expect(sessionState(line({}, { pid: 42, heldAt: '2026-11-01T09:00:00Z' })).since).toBeUndefined();
  });
});

describe('sessionState: which actions each state offers', () => {
  it('running: interrupt, pause after the turn, close', () => {
    expect(enabled(line({}, { pid: 42, busy: true }))).toEqual(sorted(['interrupt', 'pause', 'close']));
  });

  it('needs_input: pause and close, no interrupt', () => {
    expect(enabled(line({}, { pid: 42, needsInput: true }))).toEqual(sorted(['pause', 'close']));
  });

  it('ready: pause and close', () => {
    expect(enabled(line({}, { pid: 42 }))).toEqual(sorted(['pause', 'close']));
  });

  it('pause_pending: cancel the pause, interrupt the turn, close', () => {
    expect(enabled(line({}, { pid: 42, busy: true, holdPending: true }))).toEqual(sorted(['cancelPause', 'interrupt', 'close']));
  });

  it('needs_input with a queued pause: cancel the pause and close', () => {
    expect(enabled(line({}, { pid: 42, needsInput: true, holdPending: true }))).toEqual(sorted(['cancelPause', 'close']));
  });

  it('paused: resume and close', () => {
    expect(enabled(line({ state: 'held' }, { heldReason: 'user' }))).toEqual(sorted(['resume', 'close']));
  });

  it('stopped: resume and close', () => {
    expect(enabled(line({ state: 'held' }, { heldReason: 'lost' }))).toEqual(sorted(['resume', 'close']));
    expect(enabled(line({}, {}))).toEqual(sorted(['resume', 'close']));
  });

  it('closed: nothing', () => {
    expect(enabled(line({ state: 'hung-up' }))).toEqual([]);
  });

  it('a pinned session cannot be paused, and says so', () => {
    const s = sessionState(line({}, { pid: 42, pinned: true }));
    expect(s.actions.pause).toEqual({ enabled: false, reason: 'session.why.pinned' });
  });
});

describe('sessionState: why an action is disabled', () => {
  it('gives a reason for every disabled action, in every state', () => {
    const cases: LocalLine[] = [
      line({}, { pid: 42, busy: true }),
      line({}, { pid: 42, needsInput: true }),
      line({}, { pid: 42 }),
      line({}, { pid: 42, busy: true, holdPending: true }),
      line({ state: 'held' }, { heldReason: 'user' }),
      line({ state: 'held' }, { heldReason: 'lost' }),
      line({ state: 'hung-up' }),
    ];
    for (const l of cases)
      for (const a of SESSION_ACTIONS) {
        const g = sessionState(l).actions[a];
        if (!g.enabled) expect(g.reason, `${sessionState(l).state}.${a}`).toMatch(/^session\.why\./);
        else expect(g.reason).toBeUndefined();
      }
  });

  it('says the agent is not mid-turn when interrupt is disabled on a ready session', () => {
    expect(sessionState(line({}, { pid: 42 })).actions.interrupt.reason).toBe('session.why.notBusy');
  });

  it('says the pause is already queued when pause is disabled on pause_pending', () => {
    expect(sessionState(line({}, { pid: 42, busy: true, holdPending: true })).actions.pause.reason).toBe('session.why.pausePending');
  });

  it('says the session already runs when resume is disabled on a live session', () => {
    expect(sessionState(line({}, { pid: 42 })).actions.resume.reason).toBe('session.why.alive');
  });

  it('says there is no process to pause or interrupt on a paused session', () => {
    const s = sessionState(line({ state: 'held' }, { heldReason: 'user' }));
    expect(s.actions.pause.reason).toBe('session.why.noProcess');
    expect(s.actions.interrupt.reason).toBe('session.why.noProcess');
  });

  it('says the session is closed for every action of a hung-up line', () => {
    const s = sessionState(line({ state: 'hung-up' }));
    for (const a of SESSION_ACTIONS) expect(s.actions[a].reason).toBe('session.why.closed');
  });
});
