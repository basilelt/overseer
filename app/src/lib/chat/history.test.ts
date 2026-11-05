import { describe, expect, it } from 'vitest';
import { atEdge, recall, sentHistory } from './history';
import type { ChatEntry } from '../types';

const e = (kind: ChatEntry['kind'], text: string, seq: number): ChatEntry => ({ id: String(seq), seq, rev: 0, at: null, kind, text });

describe('composer history', () => {
  it('lists sent messages newest first, skipping repeats, empties and injected tags', () => {
    const entries = [e('user', 'first', 1), e('assistant', 'ok', 2), e('user', 'second', 3), e('user', 'second', 4), e('user', '<task-notification>x', 5), e('user', '  ', 6)];
    expect(sentHistory(entries)).toEqual(['second', 'first']);
  });

  it('walks back with up, forward with down and restores the draft', () => {
    const h = ['b', 'a'];
    let s = { index: -1, draft: '' };
    const up1 = recall(h, s, 'typing', 'up')!;
    expect(up1.text).toBe('b');
    s = up1.state;
    const up2 = recall(h, s, up1.text, 'up')!;
    expect(up2.text).toBe('a');
    expect(recall(h, up2.state, 'a', 'up')).toBeNull();
    const down1 = recall(h, up2.state, 'a', 'down')!;
    expect(down1.text).toBe('b');
    const down2 = recall(h, down1.state, 'b', 'down')!;
    expect(down2.text).toBe('typing');
    expect(recall(h, down2.state, 'typing', 'down')).toBeNull();
  });

  it('only takes over the arrows on the first or last line with no selection', () => {
    expect(atEdge('one\ntwo', 2, 2, 'up')).toBe(true);
    expect(atEdge('one\ntwo', 5, 5, 'up')).toBe(false);
    expect(atEdge('one\ntwo', 5, 5, 'down')).toBe(true);
    expect(atEdge('one\ntwo', 1, 2, 'up')).toBe(false);
  });
});
