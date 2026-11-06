import { describe, expect, it } from 'vitest';
import { isEnded, pausedDays, STATE_URGENCY, SESSION_ACTIONS, sessionState, type SessionAction } from './session-state';
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

  it('is attention when the agent waits for an answer, even while busy', () => {
    expect(sessionState(line({}, { pid: 42, busy: true, needsInput: true })).state).toBe('attention');
  });

  it('is open when the agent is alive and not busy', () => {
    expect(sessionState(line({ state: 'idle' }, { pid: 42 })).state).toBe('open');
  });

  it('is alive with no pid as long as it has a live Overseer terminal', () => {
    expect(sessionState(line({}, { termId: 't1' })).state).toBe('open');
    expect(sessionState(line({}, {}), 't1').state).toBe('open');
    expect(sessionState({ line: line().line }, 't1').state).toBe('open');
  });

  it('is closed when the line is active but no process or terminal backs it', () => {
    expect(sessionState(line({}, {})).state).toBe('closed');
    expect(sessionState({ line: line().line }).state).toBe('closed');
  });

  it('stays running, with a pause-after-turn sub-label, when a pause is queued on a busy agent', () => {
    const s = sessionState(line({}, { pid: 42, busy: true, holdPending: true }));
    expect([s.state, s.sub]).toEqual(['running', { key: 'session.state.pause_pending' }]);
  });

  it('is running with an "N sub-agents" sub-label when subagents run on an idle parent', () => {
    const s = sessionState(line({ state: 'idle' }, { pid: 42, subagents: 3 }));
    expect([s.state, s.sub]).toEqual(['running', { key: 'session.subagents', n: 3 }]);
  });

  it('uses the singular sub-label for one sub-agent', () => {
    expect(sessionState(line({}, { pid: 42, subagents: 1 })).sub).toEqual({ key: 'session.subagentsOne', n: 1 });
  });

  it('has no sub-label with zero or unknown subagents', () => {
    expect(sessionState(line({}, { pid: 42, subagents: 0 })).sub).toBeUndefined();
    expect(sessionState(line({}, { pid: 42, busy: true })).sub).toBeUndefined();
  });

  it('prefers the pause sub-label over the sub-agents count', () => {
    expect(sessionState(line({}, { pid: 42, subagents: 2, holdPending: true })).sub).toEqual({ key: 'session.state.pause_pending' });
  });

  it('is attention, not running, when subagents run but the agent waits for an answer', () => {
    expect(sessionState(line({}, { pid: 42, subagents: 2, needsInput: true })).state).toBe('attention');
  });

  it('is not running because of subagents once the session is paused or closed', () => {
    expect(sessionState(line({ state: 'held' }, { heldReason: 'user', subagents: 2 })).state).toBe('paused');
    expect(sessionState(line({}, { subagents: 2 })).state).toBe('closed');
  });

  it('stays attention when a pause is queued, since the user must answer first', () => {
    expect(sessionState(line({}, { pid: 42, needsInput: true, holdPending: true })).state).toBe('attention');
  });

  it('is paused when the user held it on purpose', () => {
    expect(sessionState(line({ state: 'held' }, { heldReason: 'user' })).state).toBe('paused');
  });

  it('is closed when held because its process is gone', () => {
    expect(sessionState(line({ state: 'held' }, { heldReason: 'lost' })).state).toBe('closed');
  });

  it('is closed when held without a reason, the safe reading of an old daemon', () => {
    expect(sessionState(line({ state: 'held' }, {})).state).toBe('closed');
  });

  it('flags a hung-up line as ended, whatever its process says, instead of giving it a state of its own', () => {
    const s = sessionState(line({ state: 'hung-up' }, { pid: 42, busy: true }));
    expect([s.state, s.ended]).toEqual(['closed', true]);
    expect(sessionState(line({}, { pid: 42 })).ended).toBeUndefined();
    expect(isEnded(line({ state: 'hung-up' }))).toBe(true);
    expect(isEnded(line({ state: 'held' }))).toBe(false);
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

  it('attention: pause and close, no interrupt', () => {
    expect(enabled(line({}, { pid: 42, needsInput: true }))).toEqual(sorted(['pause', 'close']));
  });

  it('open: pause and close', () => {
    expect(enabled(line({}, { pid: 42 }))).toEqual(sorted(['pause', 'close']));
  });

  it('running with a queued pause: cancel the pause, interrupt the turn, close', () => {
    expect(enabled(line({}, { pid: 42, busy: true, holdPending: true }))).toEqual(sorted(['cancelPause', 'interrupt', 'close']));
  });

  it('attention with a queued pause: cancel the pause and close', () => {
    expect(enabled(line({}, { pid: 42, needsInput: true, holdPending: true }))).toEqual(sorted(['cancelPause', 'close']));
  });

  it('paused: resume and close', () => {
    expect(enabled(line({ state: 'held' }, { heldReason: 'user' }))).toEqual(sorted(['resume', 'close']));
  });

  it('closed: resume and close', () => {
    expect(enabled(line({ state: 'held' }, { heldReason: 'lost' }))).toEqual(sorted(['resume', 'close']));
    expect(enabled(line({}, {}))).toEqual(sorted(['resume', 'close']));
  });

  it('ended: nothing', () => {
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

  it('says the agent is not mid-turn when interrupt is disabled on an open session', () => {
    expect(sessionState(line({}, { pid: 42 })).actions.interrupt.reason).toBe('session.why.notBusy');
  });

  it('says the pause is already queued when pause is disabled on a queued pause', () => {
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

  it('says the session has ended for every action of a hung-up line', () => {
    const s = sessionState(line({ state: 'hung-up' }));
    for (const a of SESSION_ACTIONS) expect(s.actions[a].reason).toBe('session.why.ended');
  });
});

describe('pausedDays', () => {
  const now = Date.parse('2026-11-05T10:00:00Z');

  it('counts whole days since the pause', () => {
    expect(pausedDays('2026-11-02T09:00:00Z', now)).toBe(3);
  });

  it('stays plain when the pause is under a day old, absent or unreadable', () => {
    expect(pausedDays('2026-11-05T01:00:00Z', now)).toBeNull();
    expect(pausedDays(undefined, now)).toBeNull();
    expect(pausedDays('nope', now)).toBeNull();
  });
});

describe('STATE_URGENCY', () => {
  it('orders the five states from most to least urgent', () => {
    expect(STATE_URGENCY).toEqual(['attention', 'running', 'open', 'paused', 'closed']);
  });
});
