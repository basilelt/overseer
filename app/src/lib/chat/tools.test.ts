import { describe, expect, it } from 'vitest';
import { clip, hasDetail, miniDiff, toolSummary } from './tools';

describe('chat tool helpers', () => {
  it('diffs old and new strings with a little context', () => {
    const d = miniDiff({ old: 'a\nb\nc\nd\ne\nf\ng', new: 'a\nb\nc\nD\ne\nf\ng' })!;
    expect(d.lines).toEqual([
      { kind: 'ctx', text: 'b' },
      { kind: 'ctx', text: 'c' },
      { kind: 'del', text: 'd' },
      { kind: 'add', text: 'D' },
      { kind: 'ctx', text: 'e' },
      { kind: 'ctx', text: 'f' },
    ]);
    expect([d.added, d.removed, d.more]).toEqual([1, 1, 0]);
  });

  it('shows a write as all additions and caps long diffs', () => {
    const d = miniDiff({ old: '', new: Array.from({ length: 50 }, (_, i) => `l${i}`).join('\n') }, 10)!;
    expect(d.lines).toHaveLength(10);
    expect(d.added).toBe(50);
    expect(d.more).toBe(40);
  });

  it('reads apply_patch bodies', () => {
    const d = miniDiff({ patch: '*** Begin Patch\n*** Update File: a.rs\n@@\n fn a() {\n-    1\n+    2\n }\n*** End Patch' })!;
    expect(d.lines.map((l) => l.kind)).toEqual(['ctx', 'del', 'add', 'ctx']);
  });

  it('clips long output', () => {
    expect(clip('1\n2\n3\n4\n', 2)).toEqual({ text: '1\n2', more: 2 });
    expect(clip('ok\n')).toEqual({ text: 'ok', more: 0 });
  });

  it('summarises steps', () => {
    expect(toolSummary({ name: 'Bash', kind: 'bash', label: 'Bash', command: 'cargo test\n--x', state: 'done' })).toBe('cargo test');
    expect(toolSummary({ name: 'Read', kind: 'read', label: 'Read a.rs', state: 'done' })).toBe('Read a.rs');
    expect(hasDetail({ name: 'Grep', kind: 'search', label: 'x', state: 'done' })).toBe(false);
    expect(miniDiff(undefined)).toBeNull();
  });
});
