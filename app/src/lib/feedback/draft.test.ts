import { afterEach, describe, expect, it } from 'vitest';
import { DRAFT_ID, feedbackDrafts, initialForm, ownsText } from './draft';

afterEach(() => feedbackDrafts.delete(DRAFT_ID));

describe('feedback form draft', () => {
  it('starts empty as a bug report', () => {
    expect(initialForm(null, undefined)).toEqual({ kind: 'bug', title: '', description: '' });
  });

  it('brings back the typed text when the sheet reopens with only a recording', () => {
    feedbackDrafts.set(DRAFT_ID, { kind: 'bug', title: 'my title', description: 'my description' });
    expect(initialForm({ kind: 'bug' }, feedbackDrafts.get(DRAFT_ID))).toEqual({ kind: 'bug', title: 'my title', description: 'my description' });
  });

  it('keeps the kind the user picked when the sheet is reopened plainly', () => {
    expect(initialForm(null, { kind: 'idea', title: 't', description: '' }).kind).toBe('idea');
  });

  it('lets a prefill that carries its own text win over the saved draft', () => {
    const saved = { kind: 'idea' as const, title: 'old', description: 'old text' };
    expect(initialForm({ kind: 'bug', title: 'Crash', description: 'report' }, saved)).toEqual({ kind: 'bug', title: 'Crash', description: 'report' });
    expect(ownsText({ title: 'Crash' })).toBe(false);
    expect(ownsText({ kind: 'bug' })).toBe(true);
    expect(ownsText(null)).toBe(true);
  });

  it('is not kept once emptied', () => {
    feedbackDrafts.set(DRAFT_ID, { kind: 'bug', title: 'a', description: '' });
    feedbackDrafts.set(DRAFT_ID, { kind: 'bug', title: ' ', description: '' });
    expect(feedbackDrafts.get(DRAFT_ID)).toBeUndefined();
  });
});
