import { describe, expect, it } from 'vitest';
import { declOf } from '../../../test/css';
import bar from './SubmitBar.svelte?raw';

describe('CSS guard: review submit bar', () => {
  it('wraps its buttons in a narrow pane instead of running under the next panel', () => {
    expect(declOf(bar, '.row', 'flex-wrap')).toBe('wrap');
  });
});
