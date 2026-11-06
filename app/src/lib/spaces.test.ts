import { describe, expect, it } from 'vitest';
import type { SidebarItem } from './grouping';
import { spaces } from './spaces';
import type { Workspace } from './types';

const ws = (id: string, name: string, over: Partial<Workspace> = {}) => ({ id, name, rank: 1, pinned: false, archived: false, ...over }) as Workspace;
const it_ = (key: string, state: SidebarItem['state'], over: Partial<SidebarItem> = {}) => ({ key, kind: 'claude', group: 'ready', state, title: key, at: '2026-11-05T10:00:00Z', ...over }) as SidebarItem;

describe('spaces', () => {
  it('splits workspaces with sessions from empty ones', () => {
    // Given one workspace with a session and one without
    // When the spaces are built
    // Then the first is a row and the second an empty chip
    const r = spaces([ws('a', 'A'), ws('b', 'B', { rank: 2 })], [it_('k1', 'open')], new Map([['k1', 'a']]), '');
    expect(r.rows.map((x) => x.id)).toEqual(['a']);
    expect(r.empty.map((w) => w.id)).toEqual(['b']);
  });

  it('counts shells out and keeps a workspace holding only shells empty', () => {
    // Given a workspace whose only item is a shell
    // When the spaces are built
    // Then it is empty
    const r = spaces([ws('a', 'A')], [it_('s', 'open', { kind: 'shell' })], new Map([['s', 'a']]), '');
    expect(r.rows).toEqual([]);
    expect(r.empty).toHaveLength(1);
  });

  it('puts the unassigned sessions in a last "other" row', () => {
    // Given a session outside any workspace
    // When the spaces are built
    // Then an other row with a null workspace comes last
    const r = spaces([ws('a', 'A')], [it_('k1', 'open'), it_('k2', 'open')], new Map([['k1', 'a']]), '');
    expect(r.rows.map((x) => x.id)).toEqual(['a', '']);
    expect(r.rows[1]!.workspace).toBeNull();
  });

  it('summarises each row by state', () => {
    // Given attention, running and paused sessions
    // When the spaces are built
    // Then the summary carries the counts and the total
    const items = [it_('k1', 'attention'), it_('k2', 'running'), it_('k3', 'paused'), it_('k4', 'closed')];
    const r = spaces([ws('a', 'A')], items, new Map(items.map((i) => [i.key, 'a'])), '');
    expect(r.rows[0]!.summary.counts).toMatchObject({ attention: 1, running: 1, paused: 1, closed: 1 });
    expect(r.rows[0]!.summary.total).toBe(4);
  });

  it('orders pinned first, then rank, and drops archived ones', () => {
    // Given a pinned, a ranked and an archived workspace, all with sessions
    // When the spaces are built
    // Then pinned leads and archived is gone
    const items = ['a', 'b', 'c'].map((k) => it_(k, 'open'));
    const r = spaces([ws('a', 'A', { rank: 1 }), ws('b', 'B', { rank: 2, pinned: true }), ws('c', 'C', { archived: true })], items, new Map(items.map((i) => [i.key, i.key])), '');
    expect(r.rows.map((x) => x.id)).toEqual(['b', 'a']);
  });

  it('filters by workspace name or session title', () => {
    // Given two workspaces
    // When filtering on a session title
    // Then only the workspace holding it remains
    const items = [it_('k1', 'open', { title: 'Fix login' }), it_('k2', 'open', { title: 'Docs' })];
    const r = spaces([ws('a', 'A'), ws('b', 'B')], items, new Map([['k1', 'a'], ['k2', 'b']]), 'login');
    expect(r.rows.map((x) => x.id)).toEqual(['a']);
    expect(r.empty).toEqual([]);
  });
});
