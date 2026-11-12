import { describe, expect, it } from 'vitest';
import { attention } from './attention';
import type { SidebarItem } from './grouping';
import { stateOfGroup } from '../test/fixtures';

const item = (key: string, group: SidebarItem['group'], at: string, over: Partial<SidebarItem> = {}): SidebarItem => ({
  key,
  kind: 'claude',
  group,
  state: stateOfGroup(group),
  title: key.toUpperCase(),
  at,
  lineId: key,
  ...over,
});

describe('attention', () => {
  it('lists only the sessions that require attention, most recent first', () => {
    const items = [item('a', 'needs_input', '2026-11-06T10:00:00Z'), item('b', 'running', '2026-11-06T12:00:00Z'), item('c', 'needs_input', '2026-11-06T11:00:00Z')];
    expect(attention(items, new Map()).map((e) => e.key)).toEqual(['c', 'a']);
  });

  it('carries the session, its title, what it last did and its workspace', () => {
    const items = [item('a', 'needs_input', '2026-11-06T10:00:00Z', { status: 'Edit src/x.ts', termId: 't1' })];
    expect(attention(items, new Map([['a', 'w1']]))).toEqual([
      { key: 'a', lineId: 'a', termId: 't1', title: 'A', detail: 'Edit src/x.ts', workspaceId: 'w1', at: '2026-11-06T10:00:00Z' },
    ]);
  });

  it('has no workspace for a session outside any', () => {
    expect(attention([item('a', 'needs_input', '2026-11-06T10:00:00Z')], new Map())[0]).toMatchObject({ workspaceId: null, detail: undefined });
  });

  it('keeps a session that needs input even when its group is needs_testing', () => {
    const it1 = item('a', 'needs_testing', '2026-11-06T10:00:00Z', { state: 'attention' });
    expect(attention([it1], new Map()).map((e) => e.key)).toEqual(['a']);
  });

  it('is empty when nothing waits, and ignores plain shells', () => {
    expect(attention([], new Map())).toEqual([]);
    expect(attention([item('s', 'needs_input', '2026-11-06T10:00:00Z', { kind: 'shell' })], new Map())).toEqual([]);
  });
});
