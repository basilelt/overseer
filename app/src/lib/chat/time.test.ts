import { describe, expect, it } from 'vitest';
import { hhmm } from './time';

describe('chat entry time', () => {
  // Given an entry timestamp
  // When it is formatted with the shared formatter
  // Then it matches the locale hour:minute format previously built per entry
  it('formats like toLocaleTimeString', () => {
    const at = '2026-10-30T14:05:00Z';
    expect(hhmm(at)).toBe(new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }));
  });

  // Given an unparsable timestamp
  // When it is formatted
  // Then it yields an empty string instead of throwing
  it('ignores invalid dates', () => {
    expect(hhmm('nope')).toBe('');
  });
});
