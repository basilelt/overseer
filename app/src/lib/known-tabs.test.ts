import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SidebarItem } from './grouping';
import { loadKnown, MAX_KEYS, MAX_KNOWN, mergeKnown, saveKnown, startKnown } from './known-tabs';
import { workspaceTabs } from './workspaces';
import { stateOfGroup } from './test-fixtures';

const item = (key: string, group: SidebarItem['group'] = 'running', over: Partial<SidebarItem> = {}): SidebarItem => ({ key, kind: 'claude', group, state: stateOfGroup(group), title: key, at: '2026-10-29T10:00:00Z', ...over });

function memoryStorage() {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
}

afterEach(() => vi.unstubAllGlobals());

describe('tabs closed before a relaunch', () => {
  it('does not bring back a closed tab and still opens a member started after the save', () => {
    vi.stubGlobal('localStorage', memoryStorage());
    const before = [item('line:a'), item('term:closed', 'running', { kind: 'shell' })];
    saveKnown('local', 'ws1', { known: [...mergeKnown(new Set(), before, before)] });

    const after = [...before, item('line:new')];
    const known = startKnown(loadKnown('local', 'ws1'), ['line:a'], after);
    expect(workspaceTabs(['line:a'], after, after, known)).toEqual(['line:a', 'line:new']);
  });

  it('keeps a held member out and a closed term out once it became a line', () => {
    const items = [item('line:a'), item('line:y', 'ready', { termId: 'x' }), item('line:h', 'paused')];
    expect(workspaceTabs([], items, items, new Set(['line:a', 'term:x']))).toEqual([]);
  });

  it('trusts the saved layout when nothing was recorded yet, and opens every member on a first visit', () => {
    const items = [item('line:a'), item('line:b')];
    expect(workspaceTabs(['line:a'], items, items, startKnown(null, ['line:a'], items))).toEqual(['line:a']);
    expect(workspaceTabs([], items, items, startKnown(null, [], items))).toEqual(['line:a', 'line:b']);
  });

  it('forgets terms that are gone, keeps closed sessions that may come back, and keeps the record per host and layout', () => {
    vi.stubGlobal('localStorage', memoryStorage());
    const items = [item('line:a', 'ready', { termId: 't' })];
    expect([...mergeKnown(new Set(['line:gone', 'term:t', 'term:dead']), [], items)]).toEqual(['line:gone', 'term:t']);
    saveKnown('local', '#other', { known: ['line:a'], tabs: ['line:a'] });
    saveKnown('ssh:box', '#other', { known: [] });
    expect(loadKnown('local', '#other')).toEqual({ known: ['line:a'], tabs: ['line:a'] });
    expect(loadKnown('ssh:box', '#other')).toEqual({ known: [] });
    expect(loadKnown('local', 'other')).toBeNull();
  });
  it('does not reopen a closed session that was hung up and later resumed under the same line', () => {
    const open = [item('line:a'), item('line:closed')];
    const whileHungUp = mergeKnown(mergeKnown(new Set(), open, open), [item('line:a')], [item('line:a')]);
    const resumed = [item('line:a'), item('line:closed')];
    expect(workspaceTabs(['line:a'], resumed, resumed, whileHungUp)).toEqual(['line:a']);
  });

  it('keeps at most MAX_KEYS keys, dropping the oldest', () => {
    const many = Array.from({ length: MAX_KEYS + 5 }, (_, i) => item(`line:${i}`));
    const known = mergeKnown(new Set(), many, many);
    expect(known.size).toBe(MAX_KEYS);
    expect(known.has('line:0')).toBe(false);
    expect(known.has(`line:${MAX_KEYS + 4}`)).toBe(true);
  });
});

describe('known tabs storage', () => {
  it('keeps only the most recently saved layouts', () => {
    vi.stubGlobal('localStorage', memoryStorage());
    for (let i = 0; i < MAX_KNOWN + 10; i++) saveKnown('local', `ws${i}`, { known: [`line:${i}`] });
    saveKnown('local', 'ws15', { known: ['line:15b'] });
    expect(loadKnown('local', 'ws0')).toBeNull();
    expect(loadKnown('local', 'ws9')).toBeNull();
    expect(loadKnown('local', 'ws10')).not.toBeNull();
    expect(loadKnown('local', `ws${MAX_KNOWN + 9}`)?.known).toEqual([`line:${MAX_KNOWN + 9}`]);
    expect(loadKnown('local', 'ws15')?.known).toEqual(['line:15b']);
    expect(Object.keys(JSON.parse(localStorage.getItem('overseer.knownTabs')!))).toHaveLength(MAX_KNOWN);
  });
});
