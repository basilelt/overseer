import { describe, expect, it } from 'vitest';
import type { ChatEntry } from '../types';
import { anchorOf, chatCache, trimmed, weigh, type ChatSnapshot } from './cache';
import { EMPTY_CHAT } from './store';

const entry = (seq: number, text = 'hi'): ChatEntry => ({ id: `e${seq}`, seq, rev: 1, at: null, kind: 'assistant', text });

function snap(n: number, opts: Partial<ChatSnapshot> = {}, text = 'hi'): ChatSnapshot {
  const entries = Array.from({ length: n }, (_, i) => entry(i + 1, text));
  return { state: { entries, cursor: n, more: false, model: null, loaded: true }, anchor: { id: 'e2', offset: -12 }, following: false, shown: n, ...opts };
}

describe('chat cache', () => {
  it('returns the cached page and scroll position for a line', () => {
    const c = chatCache();
    c.put('local|ln_1', snap(3));
    const hit = c.get('local|ln_1');
    expect(hit?.state.entries.map((e) => e.seq)).toEqual([1, 2, 3]);
    expect(hit?.anchor).toEqual({ id: 'e2', offset: -12 });
    expect(hit?.state.cursor).toBe(3);
    expect(c.get('ssh:other|ln_1')).toBeUndefined();
  });

  it('never stores a page that was not loaded', () => {
    const c = chatCache();
    expect(c.put('k', { state: EMPTY_CHAT, anchor: null, following: true, shown: 0 })).toBe(false);
    expect(c.get('k')).toBeUndefined();
  });

  it('keeps only the tail of long transcripts and follows the bottom', () => {
    const t = trimmed(snap(10), 4);
    expect(t.state.entries.map((e) => e.seq)).toEqual([7, 8, 9, 10]);
    expect(t.state.more).toBe(true);
    expect(t.state.cursor).toBe(10);
    expect(t.following).toBe(true);
    expect(t.anchor).toBeNull();
    expect(t.shown).toBe(4);
  });

  it('is bounded by line count and by size', () => {
    const c = chatCache(2, 1_000);
    c.put('a', snap(1));
    c.put('b', snap(1));
    c.put('c', snap(1));
    expect(c.size).toBe(2);
    expect(c.get('a')).toBeUndefined();
    expect(c.put('big', snap(2, {}, 'x'.repeat(600)))).toBe(false);
    expect(c.get('big')).toBeUndefined();
  });

  it('anchors on the first entry still visible at the top of the list', () => {
    const box = (top: number, bottom: number, id?: string) => ({ getBoundingClientRect: () => ({ top, bottom }), dataset: { id } });
    const items = [box(-200, -20, 'e1'), box(-20, 80, 'e2'), box(80, 300, 'e3')];
    expect(anchorOf({ ...box(0, 600), querySelectorAll: () => items })).toEqual({ id: 'e2', offset: -20 });
    expect(anchorOf({ ...box(0, 600), querySelectorAll: () => [] })).toBeNull();
  });

  it('weighs tool output and diffs', () => {
    const s = snap(1);
    s.state.entries[0].tool = { name: 'Bash', kind: 'bash', label: 'ls', state: 'done', output: 'x'.repeat(100), diff: { patch: 'y'.repeat(50) } } as never;
    expect(weigh(s)).toBe(64 + 2 + 150);
  });
});
