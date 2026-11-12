import { describe, expect, it } from 'vitest';
import * as D from './draft';
import { applySuggestion, batch, canSubmit, chips, countByFile, insertSuggestion, parseBody, previewSuggestion, reopenStatus, threadSuggestion, withStatus } from './threads';
import { unresolvedByWorktree } from './unresolved';
import type { Delivery, ReviewComment, Thread } from './types';

const c = (body: string, pending = false, author: 'user' | 'agent' = 'user'): ReviewComment => ({ id: `c${body.length}`, author, body, pending, createdAt: '', updatedAt: '' });
const d = (status: Delivery['status']): Delivery => ({ id: 'd1', status, kind: 'request_changes', target: 'term:1', at: '', threads: [] });

function thread(over: Partial<Thread> = {}): Thread {
  return { id: 't1', worktree: '/w', path: 'a.rs', side: 'new', line: 3, endLine: 4, originalLine: 3, originalEndLine: 4, code: 'a\nb', status: 'pending', outdated: false, changed: false, createdAt: '', updatedAt: '', comments: [c('x', true)], ...over };
}

describe('comment box', () => {
  it('opens on a line, extends by drag and shift-click, and closes on finish', () => {
    let s = D.click(null, 'a.rs', 'new', 10, false);
    expect(s).toMatchObject({ start: 10, end: 10, selecting: true });
    s = D.extend(s, 'a.rs', 'new', 7)!;
    expect([s.start, s.end]).toEqual([7, 10]);
    expect(D.extend(s, 'b.rs', 'new', 3)).toBe(s);
    s = D.finish(s)!;
    expect(s.selecting).toBe(false);
    s = D.click(s, 'a.rs', 'new', 14, true);
    expect([s.start, s.end, s.selecting]).toEqual([10, 14, false]);
    expect(D.covers(s, 'a.rs', 'new', 12)).toBe(true);
    expect(D.covers(s, 'a.rs', 'old', 12)).toBe(false);
  });

  it('keeps a written comment when its line is clicked again', () => {
    const s = D.setBody(D.finish(D.begin('a.rs', 'new', 5))!, 'hello');
    expect(D.click(s, 'a.rs', 'new', 5, false).body).toBe('hello');
    expect(D.click(s, 'a.rs', 'new', 6, false).body).toBe('');
  });

  it('maps keys, suggestion insertion and the new thread payload', () => {
    expect(D.keyAction({ key: 'Enter', ctrlKey: true })).toBe('submit');
    expect(D.keyAction({ key: 'Enter', metaKey: true })).toBe('submit');
    expect(D.keyAction({ key: 'Enter' })).toBeNull();
    expect(D.keyAction({ key: 'Escape' })).toBe('cancel');
    let s = D.finish(D.extend(D.begin('a.rs', 'old', 2), 'a.rs', 'old', 3))!;
    expect(D.ready(s)).toBe(false);
    s = D.suggest(D.setBody(s, 'Rename'), ['let a = 1;', 'let b = 2;']);
    expect(s.body).toBe('Rename\n\n```suggestion\nlet a = 1;\nlet b = 2;\n```');
    expect(D.ready(s)).toBe(true);
    expect(D.toNewThread(s, { lineId: 'L' })).toEqual({ lineId: 'L', path: 'a.rs', side: 'old', line: 2, endLine: 3, body: s.body });
  });
});

describe('thread states', () => {
  it('labels each status, delivery and flag', () => {
    expect(chips(thread())).toEqual(['pending']);
    expect(chips(thread({ status: 'sent', delivery: d('queued') }))).toEqual(['queued']);
    expect(chips(thread({ status: 'sent', delivery: d('delivered'), changed: true }))).toEqual(['delivered', 'changed']);
    expect(chips(thread({ status: 'addressed', outdated: true }))).toEqual(['addressed', 'outdated']);
    expect(chips(thread({ status: 'resolved' }))).toEqual(['resolved']);
  });

  it('reopens to sent once submitted, else pending', () => {
    expect(reopenStatus(thread({ status: 'resolved' }))).toBe('pending');
    expect(reopenStatus(thread({ status: 'resolved', comments: [c('x')] }))).toBe('sent');
    expect(withStatus(thread({ status: 'addressed', delivery: d('delivered') }), 'resolve').status).toBe('resolved');
    expect(withStatus(thread({ status: 'resolved', delivery: d('delivered') }), 'reopen').status).toBe('sent');
  });

  it('counts unresolved threads per worktree and per file', () => {
    const ts = [thread(), thread({ id: 't2', status: 'resolved' }), thread({ id: 't3', status: 'addressed', worktree: '/v', path: 'b.rs' })];
    expect(unresolvedByWorktree(ts)).toEqual({ '/w': 1, '/v': 1 });
    expect(countByFile(ts)).toEqual({ 'a.rs': 1, 'b.rs': 1 });
  });
});

describe('suggestions', () => {
  it('parses the body into paragraphs, code and suggestion blocks', () => {
    const blocks = parseBody('Use `b2` here.\n\n```rust\nfn x() {}\n```\n```suggestion\n    b2();\n```');
    expect(blocks).toEqual([
      { type: 'p', inlines: [{ code: false, text: 'Use ' }, { code: true, text: 'b2' }, { code: false, text: ' here.' }] },
      { type: 'code', lang: 'rust', text: 'fn x() {}' },
      { type: 'suggestion', lines: ['    b2();'] },
    ]);
  });

  it('takes the last user suggestion of a thread', () => {
    const t = thread({ comments: [c('```suggestion\none\n```'), c('```suggestion\ntwo\n```', false, 'agent'), c('no code')] });
    expect(threadSuggestion(t)).toEqual(['one']);
    expect(threadSuggestion(thread())).toBeNull();
    expect(insertSuggestion('', ['x'])).toBe('```suggestion\nx\n```');
  });

  it('previews and applies a suggestion in place of the commented lines', () => {
    expect(previewSuggestion(['a', 'b', 'c'], ['a', 'B', 'c'])).toEqual([
      { kind: 'ctx', text: 'a' },
      { kind: 'del', text: 'b' },
      { kind: 'add', text: 'B' },
      { kind: 'ctx', text: 'c' },
    ]);
    expect(previewSuggestion(['a'], [])).toEqual([{ kind: 'del', text: 'a' }]);
    expect(applySuggestion(['1', '2', '3', '4'], 2, 3, ['x'])).toEqual(['1', 'x', '4']);
  });
});

describe('submit batching', () => {
  it('batches every pending comment, new threads and replies, grouped by file', () => {
    const ts = [
      thread(),
      thread({ id: 't2', path: 'b.rs', status: 'sent', comments: [c('old'), c('reply', true)] }),
      thread({ id: 't3', path: 'a.rs', comments: [c('one', true), c('two!', true)] }),
      thread({ id: 't4', status: 'resolved', comments: [c('done')] }),
    ];
    const b = batch(ts);
    expect(b).toEqual({ comments: 4, threads: 3, files: [{ path: 'a.rs', count: 3 }, { path: 'b.rs', count: 1 }] });
    expect(canSubmit('request_changes', b, '')).toBe(true);
    const empty = batch([ts[3]]);
    expect(canSubmit('comment', empty, '')).toBe(false);
    expect(canSubmit('comment', empty, 'LGTM overall')).toBe(true);
    expect(canSubmit('approve', empty, '')).toBe(true);
  });
});
