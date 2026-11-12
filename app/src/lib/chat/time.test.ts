import { describe, expect, it } from 'vitest';
import { hhmm } from './time';

describe('chat entry time', () => {
  it('formats like toLocaleTimeString', () => {
    const at = '2026-10-30T14:05:00Z';
    expect(hhmm(at)).toBe(new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }));
  });

  it('ignores invalid dates', () => {
    expect(hhmm('nope')).toBe('');
  });
});
