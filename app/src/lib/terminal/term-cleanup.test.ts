import { describe, expect, it } from 'vitest';
import { exitedTerm } from '../tabs/tabs';

describe('closing a tab frees an exited terminal', () => {
  const terms = [
    { id: 'a', exited: true },
    { id: 'b', exited: false },
  ];
  it('only for a terminal tab whose process has exited', () => {
    expect(exitedTerm('term:a', terms)).toBe('a');
    expect(exitedTerm('term:b', terms)).toBeNull();
    expect(exitedTerm('term:zz', terms)).toBeNull();
    expect(exitedTerm('line:a', terms)).toBeNull();
  });
});
