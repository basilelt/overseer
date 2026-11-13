import { describe, expect, it } from 'vitest';
import { stalled } from './close-force';

const ok = { ok: true };
const no = { ok: false, detail: 'x' };

describe('stalled', () => {
  it('says waiting when the agent waiting for the user is among the turn blockers', () => {
    expect(stalled({ predicates: { alive: ok, notNeedsInput: no } })).toBe('waiting');
    expect(stalled({ predicates: { atPrompt: no, notNeedsInput: no } })).toBe('waiting');
  });

  it('says turn when only a mid-turn or running-tool check fails', () => {
    expect(stalled({ predicates: { atPrompt: no } })).toBe('turn');
    expect(stalled({ predicates: { atPrompt: no, noToolRunning: no } })).toBe('turn');
    expect(stalled({ predicates: { noToolRunning: no } })).toBe('turn');
  });

  it('stays blocked when a check that protects data also fails', () => {
    expect(stalled({ predicates: { notNeedsInput: no, notPinned: no } })).toBeNull();
    expect(stalled({ predicates: { atPrompt: no, inputEmpty: no } })).toBeNull();
    expect(stalled({ predicates: { noToolRunning: no, alive: no } })).toBeNull();
  });

  it('stays blocked for other single blockers, errors, loading and no plan', () => {
    expect(stalled({ predicates: { notPinned: no } })).toBeNull();
    expect(stalled({ predicates: { inputEmpty: no } })).toBeNull();
    expect(stalled({ predicates: { notNeedsInput: no }, error: 'boom' })).toBeNull();
    expect(stalled('loading')).toBeNull();
    expect(stalled(undefined)).toBeNull();
  });

  it('is null when every check passes', () => {
    expect(stalled({ predicates: { notNeedsInput: ok, atPrompt: ok } })).toBeNull();
  });
});
