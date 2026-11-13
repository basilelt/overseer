import { describe, expect, it } from 'vitest';
import { waitingOnly } from './close-force';

const ok = { ok: true };
const no = { ok: false, detail: 'x' };

describe('waitingOnly', () => {
  it('is true when the agent waiting for the user is the only blocker', () => {
    expect(waitingOnly({ predicates: { alive: ok, notNeedsInput: no } })).toBe(true);
  });

  it('is false when a check that protects data also fails', () => {
    expect(waitingOnly({ predicates: { notNeedsInput: no, notPinned: no } })).toBe(false);
    expect(waitingOnly({ predicates: { notNeedsInput: no, inputEmpty: no } })).toBe(false);
  });

  it('is false for other single blockers, errors, loading and no plan', () => {
    expect(waitingOnly({ predicates: { notPinned: no } })).toBe(false);
    expect(waitingOnly({ predicates: { notNeedsInput: no }, error: 'boom' })).toBe(false);
    expect(waitingOnly('loading')).toBe(false);
    expect(waitingOnly(undefined)).toBe(false);
  });

  it('is false when every check passes', () => {
    expect(waitingOnly({ predicates: { notNeedsInput: ok } })).toBe(false);
  });
});
