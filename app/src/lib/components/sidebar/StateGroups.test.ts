import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { SidebarItem } from '../../session/grouping';
import type { WorkspaceGroup } from '../../session/by-state';
import StateGroups from './StateGroups.svelte';
import src from './StateGroups.svelte?raw';

const NOW = Date.parse('2026-11-05T10:05:00Z');
const item = (key: string, over: Partial<SidebarItem> = {}) => ({ key, kind: 'claude', group: 'ready', state: 'open', title: `T-${key}`, at: '2026-11-05T10:00:00Z', ...over }) as SidebarItem;
const groups: WorkspaceGroup[] = [
  { workspaceId: 'w1', items: [item('a'), item('b')], at: '' },
  { workspaceId: null, items: [item('c')], at: '' },
];
const html = (extra: Record<string, unknown> = {}) =>
  render(StateGroups, { props: { groups, names: new Map([['w1', 'webshop']]), now: NOW, selectedKey: null, onselect: () => {}, ...extra } }).body;

describe('StateGroups', () => {
  it('draws one frame per workspace, the unassigned sessions last under "Other sessions"', () => {
    // Given a workspace group and a group without workspace
    // When it renders
    // Then two frames, named, in that order
    const h = html();
    expect(h.match(/class="frame/g)).toHaveLength(2);
    expect(h.indexOf('>webshop<')).toBeLessThan(h.indexOf('>Other sessions<'));
  });

  it('lists the sessions as rows in the given order', () => {
    // Given sessions a, b, c
    // When it renders
    // Then rows follow the input order
    const h = html();
    expect(h.indexOf('T-a')).toBeLessThan(h.indexOf('T-b'));
    expect(h.indexOf('T-b')).toBeLessThan(h.indexOf('T-c'));
  });

  it('forwards the click and the menu of every row', () => {
    // Given the source
    // When read
    // Then the handlers are wired
    expect(src).toMatch(/onselect=\{\(e\) => onselect\(it\.key, e\)\}/);
    expect(src).toMatch(/onmenu=/);
  });
});
