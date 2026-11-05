import { afterEach, describe, expect, it, vi } from 'vitest';
import { isSince, loadBase, saveBase } from './base';

function memoryStorage() {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
}

afterEach(() => vi.unstubAllGlobals());

describe('base persistence', () => {
  it('remembers the base per worktree and forgets it on reset', () => {
    vi.stubGlobal('localStorage', memoryStorage());
    saveBase('/wt/a', 'develop');
    saveBase('/wt/b', 'origin/release/next');
    expect(loadBase('/wt/a')).toBe('develop');
    expect(loadBase('/wt/b')).toBe('origin/release/next');
    saveBase('/wt/a', null);
    expect(loadBase('/wt/a')).toBeNull();
    expect(loadBase('/wt/b')).toBe('origin/release/next');
    expect(loadBase(null)).toBeNull();
  });

  it('survives broken or missing storage', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => '[1,2',
      setItem: () => {
        throw new Error('quota');
      },
    });
    expect(loadBase('/wt/a')).toBeNull();
    expect(() => saveBase('/wt/a', 'main')).not.toThrow();
    vi.stubGlobal('localStorage', undefined);
    expect(loadBase('/wt/a')).toBeNull();
  });

  it('tells a since-commit base apart', () => {
    expect(isSince('abc123~1')).toBe(true);
    expect(isSince('main')).toBe(false);
    expect(isSince(null)).toBe(false);
  });
});
