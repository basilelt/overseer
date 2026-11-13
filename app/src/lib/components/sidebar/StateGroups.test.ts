import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { SidebarItem } from '../../session/grouping';
import type { WorkspaceGroup } from '../../session/by-state';
import StateGroups from './StateGroups.svelte';

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
    const h = html();
    expect(h.match(/class="frame/g)).toHaveLength(2);
    expect(h.indexOf('>webshop<')).toBeLessThan(h.indexOf('>Other sessions<'));
  });

  it('lists the sessions as rows in the given order', () => {
    const h = html();
    expect(h.indexOf('T-a')).toBeLessThan(h.indexOf('T-b'));
    expect(h.indexOf('T-b')).toBeLessThan(h.indexOf('T-c'));
  });
});
