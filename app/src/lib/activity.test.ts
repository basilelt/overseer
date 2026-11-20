import { describe, expect, it } from 'vitest';
import { ACTIVITY_BACKGROUND_MS, ACTIVITY_MS, activityDelay, plainSnippet } from './activity';

describe('activity refetch delay', () => {
  it('is quick only for the session shown in the side panel of a visible window', () => {
    expect(activityDelay(true, true)).toBe(ACTIVITY_MS);
    expect(activityDelay(false, true)).toBe(ACTIVITY_BACKGROUND_MS);
    expect(activityDelay(true, false)).toBe(ACTIVITY_BACKGROUND_MS);
    expect(ACTIVITY_BACKGROUND_MS).toBeGreaterThanOrEqual(2000);
  });
});

describe('plainSnippet', () => {
  it('shows link text, not markdown', () => {
    expect(plainSnippet('Here: [docs](http://a.b/c) and **bold** `code`')).toBe('Here: docs and bold code');
    expect(plainSnippet('![shot](a.png) plain')).toBe('shot plain');
    expect(plainSnippet('[unclosed](x')).toBe('[unclosed](x');
  });
});
