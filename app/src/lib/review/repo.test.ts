import { describe, expect, it } from 'vitest';
import { inRepo } from './repo';

describe('inRepo', () => {
  it('is false for a line the daemon could not place in a repository', () => {
    expect(inRepo({ repo: 'unknown' })).toBe(false);
  });

  it('is true for a line with a repository', () => {
    expect(inRepo({ repo: 'github.com/basilelt/overseer' })).toBe(true);
    expect(inRepo({ repo: '' })).toBe(true);
  });

  it('is true while the line is not known yet or carries no repo field', () => {
    expect(inRepo(undefined)).toBe(true);
    expect(inRepo({})).toBe(true);
  });
});
