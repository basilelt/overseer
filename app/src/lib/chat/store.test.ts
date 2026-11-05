import { describe, expect, it } from 'vitest';
import type { ChatEntry, ChatPage } from '../types';
import { applyEarlier, applyTail, applyUpdate, EMPTY_CHAT, KEEP, needsFetch } from './store';

const e = (id: string, seq: number, rev = seq, text = id): ChatEntry => ({ id, seq, rev, at: null, kind: 'assistant', text });
const page = (entries: ChatEntry[], o: Partial<ChatPage> = {}): ChatPage => ({ lineId: 'l', agent: 'claude', cursor: Math.max(0, ...entries.map((x) => x.rev)), reset: false, more: false, model: null, entries, ...o });

describe('chat store', () => {
  it('loads a tail and keeps the cursor', () => {
    const s = applyTail(EMPTY_CHAT, page([e('a', 1), e('b', 2)], { more: true, model: 'opus' }));
    expect(s.entries.map((x) => x.id)).toEqual(['a', 'b']);
    expect(s).toMatchObject({ cursor: 2, more: true, model: 'opus', loaded: true });
  });

  it('upserts changed entries by id and appends new ones in seq order', () => {
    let s = applyTail(EMPTY_CHAT, page([e('a', 1), e('t', 2, 2, 'running')]));
    s = applyUpdate(s, page([e('t', 2, 5, 'done'), e('c', 3, 4)]));
    expect(s.entries.map((x) => `${x.id}:${x.text}`)).toEqual(['a:a', 't:done', 'c:c']);
    expect(s.cursor).toBe(5);
  });

  it('ignores stale revisions', () => {
    let s = applyTail(EMPTY_CHAT, page([e('t', 1, 9, 'new')]));
    s = applyUpdate(s, page([e('t', 1, 3, 'old')], { cursor: 3 }));
    expect(s.entries[0].text).toBe('new');
    expect(s.cursor).toBe(9);
  });

  it('drops everything on reset', () => {
    let s = applyTail(EMPTY_CHAT, page([e('a', 1), e('b', 2)]));
    s = applyUpdate(s, page([e('x', 1)], { reset: true, cursor: 1 }));
    expect(s.entries.map((x) => x.id)).toEqual(['x']);
    expect(s.cursor).toBe(1);
  });

  it('caps retained entries keeping the newest and flags more', () => {
    const many = Array.from({ length: 10 }, (_, i) => e(`m${i}`, i));
    const s = applyUpdate(applyTail(EMPTY_CHAT, page([])), page(many), 4);
    expect(s.entries.map((x) => x.id)).toEqual(['m6', 'm7', 'm8', 'm9']);
    expect(s.more).toBe(true);
  });

  it('prepends an earlier page', () => {
    let s = applyTail(EMPTY_CHAT, page([e('c', 3), e('d', 4)], { more: true }));
    s = applyEarlier(s, page([e('a', 1), e('b', 2), e('c', 3)], { more: false }));
    expect(s.entries.map((x) => x.id)).toEqual(['a', 'b', 'c', 'd']);
    expect(s.more).toBe(false);
  });

  it('fetches only when the event cursor is ahead', () => {
    const s = applyTail(EMPTY_CHAT, page([e('a', 1, 7)]));
    expect(needsFetch(EMPTY_CHAT, 0)).toBe(true);
    expect(needsFetch(s, 7)).toBe(false);
    expect(needsFetch(s, 8)).toBe(true);
    expect(needsFetch(s, 0)).toBe(true);
  });

  // Given a window already at its limit while the user reads history
  // When an earlier page is prepended
  // Then the newest entries are dropped and the window is flagged as cut
  it('flags the window as cut when earlier pages push out the newest', () => {
    const many = Array.from({ length: KEEP * 2 }, (_, i) => e(`n${i}`, 100 + i));
    const s = applyEarlier(applyTail(EMPTY_CHAT, page(many), KEEP * 2), page([e('o1', 1), e('o2', 2)], { more: true }));
    expect(s.entries.length).toBe(KEEP * 2);
    expect(s.entries[0].id).toBe('o1');
    expect(s.cut).toBe(true);
  });

  // Given a cut window (its newest entries were dropped)
  // When live entries arrive
  // Then they are not appended after the gap, but visible entries still update
  it('does not append past the gap of a cut window', () => {
    const s0 = { ...applyTail(EMPTY_CHAT, page([e('a', 1), e('b', 2, 2, 'run')])), cut: true };
    const s = applyUpdate(s0, page([e('b', 2, 9, 'done'), e('z', 50, 10)]), KEEP, false);
    expect(s.entries.map((x) => `${x.id}:${x.text}`)).toEqual(['a:a', 'b:done']);
    expect(s.cut).toBe(true);
  });

  // Given a scrolled-up window larger than KEEP
  // When a live update arrives
  // Then the oldest entries the user is reading are kept
  it('keeps the read history of a scrolled-up window', () => {
    const many = Array.from({ length: KEEP + 100 }, (_, i) => e(`n${i}`, i + 1));
    const s0 = applyTail(EMPTY_CHAT, page(many), KEEP * 2);
    const s = applyUpdate(s0, page([e('new', KEEP + 500)]), KEEP, false);
    expect(s.entries[0].id).toBe('n0');
    expect(s.entries.at(-1)!.id).toBe('new');
  });

  // Given a scrolled-up window already at its hard limit
  // When live entries arrive
  // Then the oldest are kept, the newest dropped and the window flagged as cut
  it('cuts the newest of a full scrolled-up window', () => {
    const many = Array.from({ length: KEEP * 2 }, (_, i) => e(`n${i}`, i + 1));
    const s0 = applyTail(EMPTY_CHAT, page(many), KEEP * 2);
    const s = applyUpdate(s0, page([e('new', KEEP * 3)]), KEEP, false);
    expect(s.entries.length).toBe(KEEP * 2);
    expect(s.entries[0].id).toBe('n0');
    expect(s.cut).toBe(true);
  });

  // Given a cut window
  // When a fresh tail is loaded
  // Then the cut flag is cleared
  it('clears the cut flag on a fresh tail', () => {
    const s = applyTail({ ...EMPTY_CHAT, cut: true }, page([e('a', 1)]));
    expect(s.cut).toBe(false);
  });
});
