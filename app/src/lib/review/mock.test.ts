import { afterEach, describe, expect, it, vi } from 'vitest';
import { MockDaemon } from '../daemon/mock';
import type { DaemonEvent } from '../types';
import type { ReviewGraph } from './types';
import { reviewApi } from './api';
import { escapeHtml, highlightLines, langFor, splitHtml } from './highlight';
import { itemWorktree, parseReviewKey, reviewKey, titleOf } from './keys';
import { MOCK_WORKTREE } from './mock';

describe('review keys', () => {
  it('round-trips targets and resolves worktrees and titles', () => {
    for (const t of [{ worktree: '/w/x:y' }, { lineId: 'vm-01' }, { taskId: 'T1' }]) expect(parseReviewKey(reviewKey(t))).toEqual(t);
    expect(parseReviewKey('line:abc')).toBeNull();
    const k = {
      lines: [{ line: { id: 'L', worktreePath: '/w/a', branch: 'feat/x' } }],
      tasks: [{ id: 'T', number: 7, title: 'Fix', lineId: 'L' }],
      terms: [{ id: 'M', cwd: '/w/b' }],
    };
    expect(itemWorktree('review:task:T', k)).toBe('/w/a');
    expect(itemWorktree('line:L', k)).toBe('/w/a');
    expect(itemWorktree('term:M', k)).toBe('/w/b');
    expect(titleOf({ lineId: 'L' }, k)).toBe('#7 Fix');
    expect(titleOf({ worktree: '/w/c/' }, k)).toBe('c');
  });
});

describe('highlighting helpers', () => {
  it('maps languages and keeps spans balanced per line', () => {
    expect(langFor('src/a.rs')).toBe('rust');
    expect(langFor('app/X.svelte')).toBe('xml');
    expect(langFor('Dockerfile')).toBe('dockerfile');
    expect(langFor('LICENSE')).toBeNull();
    expect(splitHtml('<span class="hljs-comment">/* a\nb */</span> x\ny')).toEqual(['<span class="hljs-comment">/* a</span>', '<span class="hljs-comment">b */</span> x', 'y']);
    expect(escapeHtml(`<a href="x">&'`)).toBe('&lt;a href=&quot;x&quot;&gt;&amp;&#39;');
  });

  it('splits real highlighter output back into one balanced line per source line', async () => {
    const out = await highlightLines(['/* a', 'b */ fn main() {', '  let x = "y";', '}'], 'a.rs');
    expect(out).toHaveLength(4);
    expect(out![0]).toContain('hljs-comment');
    expect(out![1]).toMatch(/^<span class="hljs-comment">b \*\/<\/span>/);
    expect(out![2]).toContain('hljs-string');
    for (const l of out!) expect(l.match(/<span/g)?.length ?? 0).toBe(l.match(/<\/span>/g)?.length ?? 0);
  });
});

describe('mock review daemon', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('refuses the line diff and the related worktrees for a session outside a repository like the daemon', async () => {
    const d = new MockDaemon({ live: false });
    const lines = (d as unknown as { lines: { line: { id: string; repo: string; worktreePath: string } }[] }).lines;
    const plain = structuredClone(lines[0]);
    plain.line.id = 'ln_plain';
    plain.line.repo = 'unknown';
    plain.line.worktreePath = '/home/alice/work';
    lines.push(plain);
    await expect(d.request('GET', '/v1/lines/ln_plain/diff')).rejects.toMatchObject({ status: 409 });
    await expect(reviewApi(d).related({ lineId: 'ln_plain' })).rejects.toMatchObject({ status: 400, message: '/home/alice/work is not in a git worktree' });
    await expect(d.request('GET', '/v1/lines/ln_03/diff')).resolves.toMatchObject({ lineId: 'ln_03' });
    await expect(reviewApi(d).related({ lineId: 'ln_03' })).resolves.toBeTruthy();
  });

  it('gives every demo review task a non-empty diff', async () => {
    const d = new MockDaemon({ live: false });
    const api = reviewApi(d);
    const tasks = await d.request<{ id: string; number: number; status: string; lineId?: string | null; worktreePath?: string; branch?: string | null }[]>('GET', '/v1/tasks');
    const reviewable = tasks.filter((t) => (t.status === 'in_testing' || t.status === 'in_progress') && (t.lineId || t.worktreePath) && t.branch !== 'main');
    expect(reviewable.length).toBeGreaterThanOrEqual(4);
    for (const t of reviewable) {
      const diff = await api.diff({ taskId: t.id });
      expect(diff.files.length, `#${t.number}`).toBeGreaterThan(0);
      expect(diff.commits?.length, `#${t.number}`).toBeGreaterThan(0);
    }
  });

  it('serves the diff, threads in every status and a submit that gets delivered', async () => {
    const d = new MockDaemon({ live: false });
    const events: DaemonEvent[] = [];
    d.subscribe((e) => events.push(e));
    const api = reviewApi(d);
    const diff = await api.diff({ lineId: 'ln_03' });
    expect(diff.worktree).toBe(MOCK_WORKTREE);
    expect(diff.files.map((f) => f.status).sort()).toEqual(['deleted', 'modified', 'modified', 'modified', 'renamed', 'submodule', 'untracked', 'untracked']);
    const threads = await api.threads({ taskId: 'tk_10' });
    expect(new Set(threads.map((t) => t.status))).toEqual(new Set(['pending', 'sent', 'addressed', 'resolved']));
    expect(threads.some((t) => t.changed) && threads.some((t) => t.outdated)).toBe(true);

    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const created = await api.createThread({ worktree: MOCK_WORKTREE, path: 'src/term.rs', line: 30, endLine: 30, body: 'why?' });
    expect(created.status).toBe('pending');
    expect(created.code).toBe('        self.pending.push((cols, rows), Instant::now());');
    const r = await api.submit({ worktree: MOCK_WORKTREE }, 'request_changes', 'thanks');
    expect(r.delivery.status).toBe('queued');
    expect(r.threads.map((t) => t.status)).toEqual(['sent', 'sent']);
    expect(r.text).toContain('src/term.rs:30');
    expect(r.text).toContain('Overall: thanks');
    expect((await api.threads({ worktree: MOCK_WORKTREE })).every((t) => t.comments.every((c) => !c.pending))).toBe(true);
    expect(events.filter((e) => e.type === 'review.delivery').map((e) => e.data.delivery.status)).toEqual(['queued']);
    await vi.advanceTimersByTimeAsync(1599);
    expect(events.filter((e) => e.type === 'review.delivery')).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(events.filter((e) => e.type === 'review.delivery').map((e) => e.data.delivery.status)).toEqual(['queued', 'delivered']);
    d.stop();
  });

  it('applies a suggestion and reports the change', async () => {
    const d = new MockDaemon({ live: false });
    const events: DaemonEvent[] = [];
    d.subscribe((e) => events.push(e));
    const api = reviewApi(d);
    const t = (await api.threads({ worktree: MOCK_WORKTREE })).find((x) => x.path === 'app/src/lib/terminal.ts' && !x.outdated)!;
    const applied = await api.apply(t.id);
    expect(applied.status).toBe('resolved');
    const f = await api.file(MOCK_WORKTREE, 'app/src/lib/terminal.ts');
    expect(f.lines[9]).toBe('  const cols = Math.floor(r.width / cell.cols);');
    expect(events.some((e) => e.type === 'review.changed')).toBe(true);
    const outdated = (await api.threads({ worktree: MOCK_WORKTREE })).find((x) => x.outdated)!;
    await expect(api.apply(outdated.id)).rejects.toThrow();
    d.stop();
  });

  it('tags moves, formatting, tests and logic and serves a review graph', async () => {
    const d = new MockDaemon({ live: false });
    const api = reviewApi(d);
    const diff = await api.diff({ worktree: MOCK_WORKTREE });
    const f = (p: string) => diff.files.find((x) => x.path === p)!;
    const kinds = (p: string) => f(p).hunks.flatMap((h) => h.tags ?? []).map((t) => t.kind);
    const strings = f('src/util/strings.rs').hunks.map((h) => h.tags!.map((t) => t.kind));
    expect(strings).toEqual([['move'], ['format']]);
    expect(f('src/util/strings.rs').hunks[0].tags![0].ref).toEqual({ path: 'src/text.rs', hunk: 0 });
    expect(f('src/util/strings.rs').risk).toBe(0);
    expect(kinds('tests/resize.rs')).toContain('test');
    expect(kinds('tests/resize.rs')).not.toContain('logic');
    expect(kinds('src/term.rs')).toContain('logic');
    expect(Math.max(...diff.files.map((x) => x.risk ?? 0))).toBe(100);
    expect(diff.triage!.changed).toBe(Object.values(diff.triage!.byKind).reduce((a, b) => a + b!, 0));
    expect(diff.triage!.lowRiskPct).toBeGreaterThan(0);
    const g = await d.request<ReviewGraph>('GET', `/v1/review/graph?worktree=${encodeURIComponent(MOCK_WORKTREE)}`);
    expect(g.revision).toBe(diff.revision);
    expect(new Set(g.order)).toEqual(new Set(g.nodes.map((n) => n.id)));
    expect(g.nodes.find((n) => n.name === 'title')?.path).toBe('src/text.rs');
    expect(g.edges).toContainEqual({ from: 'src/text.rs::title', to: 'src/text.rs::collapse_spaces', kind: 'uses' });
    const order = g.order.indexOf.bind(g.order);
    expect(order('src/text.rs::collapse_spaces')).toBeLessThan(order('src/text.rs::title'));
    d.stop();
  });
});
