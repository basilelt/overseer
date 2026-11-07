import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { clearComposerHeight, reportComposerHeight } from './composer-height';

const props = new Map<string, string>();
const value = () => props.get('--composer-h') ?? '';

beforeEach(() => {
  props.clear();
  vi.stubGlobal('document', {
    documentElement: {
      style: {
        setProperty: (k: string, v: string) => props.set(k, v),
        removeProperty: (k: string) => props.delete(k),
      },
    },
  });
});

describe('composer height registry', () => {
  const a = Symbol('a');
  const b = Symbol('b');
  afterEach(() => {
    clearComposerHeight(a);
    clearComposerHeight(b);
    vi.unstubAllGlobals();
  });

  it('publishes the height of a single composer', () => {
    reportComposerHeight(a, 90);
    expect(value()).toBe('90px');
  });

  it('publishes the tallest of several composers', () => {
    reportComposerHeight(a, 90);
    reportComposerHeight(b, 140);
    expect(value()).toBe('140px');
    reportComposerHeight(b, 60);
    expect(value()).toBe('90px');
  });

  it('falls back to the remaining composer when one unmounts', () => {
    reportComposerHeight(a, 90);
    reportComposerHeight(b, 140);
    clearComposerHeight(b);
    expect(value()).toBe('90px');
  });

  it('removes the property when no composer is left', () => {
    reportComposerHeight(a, 90);
    clearComposerHeight(a);
    expect(value()).toBe('');
  });

  it('ignores a zero height', () => {
    reportComposerHeight(a, 0);
    expect(value()).toBe('');
  });
});
