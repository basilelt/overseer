import { describe, expect, it } from 'vitest';
import type { SidebarItem } from './grouping';
import { hostFor, spaces, toggleOpen } from './spaces';
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
    expect(r.rows.map((x) => x.id)).toEqual(['b', 'a', '']);
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

  it('moves the sessions of an archived workspace to the other row', () => {
    // Given a session whose workspace is archived
    // When the spaces are built
    // Then it is listed under other, not lost
    const r = spaces([ws('a', 'A', { archived: true })], [it_('k1', 'open')], new Map([['k1', 'a']]), '');
    expect(r.rows.map((x) => x.id)).toEqual(['']);
    expect(r.rows[0]!.items).toHaveLength(1);
  });

  it('hides ghost workspaces, as the tree does', () => {
    // Given an auto workspace whose folder is gone
    // When the spaces are built
    // Then it is neither a row nor an empty chip
    const r = spaces([ws('g', 'G', { auto: true, missing: true } as Partial<Workspace>)], [], new Map(), '');
    expect(r.rows).toEqual([]);
    expect(r.empty).toEqual([]);
  });

  it('shows all sessions when the workspace name matches, only the matching ones otherwise', () => {
    // Given two sessions in one workspace
    // When filtering by the name, then by one title
    // Then the first keeps both and the second only the match
    const items = [it_('k1', 'open', { title: 'Fix login' }), it_('k2', 'running', { title: 'Docs' })];
    const wsOf = new Map([['k1', 'a'], ['k2', 'a']]);
    const byName = spaces([ws('a', 'webshop')], items, wsOf, 'WEB');
    expect(byName.rows[0]!.items).toHaveLength(2);
    const byTitle = spaces([ws('a', 'webshop')], items, wsOf, 'login');
    expect(byTitle.rows[0]!.items.map((i) => i.key)).toEqual(['k1']);
    expect(byTitle.rows[0]!.summary.total).toBe(1);
  });

  it('filters the other row by session title', () => {
    // Given sessions outside any workspace
    // When filtering on one title
    // Then the other row holds only that one, and vanishes with no match
    const items = [it_('k1', 'open', { title: 'Fix login' }), it_('k2', 'open', { title: 'Docs' })];
    expect(spaces([], items, new Map(), 'docs').rows[0]!.items.map((i) => i.key)).toEqual(['k2']);
    expect(spaces([], items, new Map(), 'zzz').rows).toEqual([]);
  });

  it('returns nothing for no workspaces and no items', () => {
    // Given nothing
    // When built
    // Then both lists are empty
    expect(spaces([], [], new Map(), '')).toEqual({ rows: [], empty: [] });
  });

  it('breaks a rank tie by name', () => {
    // Given two workspaces of equal rank and pin
    // When built
    // Then they are ordered by name
    const items = [it_('1', 'open'), it_('2', 'open')];
    const r = spaces([ws('z', 'Zed'), ws('y', 'Alpha')], items, new Map([['1', 'z'], ['2', 'y']]), '');
    expect(r.rows.map((x) => x.name)).toEqual(['Alpha', 'Zed']);
  });

  it('lists an empty workspace only when its name matches the filter', () => {
    // Given an empty workspace
    // When filtering on another word, then on its name
    // Then it is hidden, then shown
    expect(spaces([ws('b', 'Billing')], [], new Map(), 'zzz').empty).toEqual([]);
    expect(spaces([ws('b', 'Billing')], [], new Map(), 'bill').empty).toHaveLength(1);
  });

  it('hostFor: nothing for local, a chip target for ssh and direct', () => {
    // Given the three connection kinds
    // When the host is derived
    // Then only remote ones yield a name, with the connection status
    const base = { phase: 'connected' as const, attempt: 2, version: '1.0' };
    expect(hostFor({ ...base, target: { kind: 'local', label: 'Local' } })).toBeUndefined();
    expect(hostFor({ ...base, target: { kind: 'ssh', host: 'devbox', label: 'Dev' } })).toEqual({ name: 'devbox', phase: 'connected', attempt: 2, version: '1.0' });
    expect(hostFor({ ...base, target: { kind: 'ssh', label: 'Dev' } })!.name).toBe('Dev');
    expect(hostFor({ ...base, target: { kind: 'direct', host: 'daemon.example', label: 'https://daemon.example' } })!.name).toBe('daemon.example');
    expect(hostFor({ ...base, target: { kind: 'direct', label: 'https://x' } })!.name).toBe('https://x');
  });

  it('toggleOpen: toggles an id, falls back to the current workspace and prunes gone ones', () => {
    // Given a saved list holding a deleted workspace
    // When toggling another id
    // Then the deleted one is pruned and the toggle applied; unset saves start from the fallback
    const known = new Set(['a', 'b', '']);
    expect(toggleOpen(['a', 'gone'], 'b', known, 'a')).toEqual(['a', 'b']);
    expect(toggleOpen(['a', 'b'], 'a', known, 'a')).toEqual(['b']);
    expect(toggleOpen(undefined, 'b', known, 'a')).toEqual(['a', 'b']);
    expect(toggleOpen(undefined, 'a', known, 'a')).toEqual([]);
  });

  it('toggleOpen never prunes while the workspace list is empty', () => {
    // Given workspaces still loading (only the other row is known)
    // When toggling
    // Then the saved ids survive
    expect(toggleOpen(['a', 'b'], 'c', new Set(['']), 'a')).toEqual(['a', 'b', 'c']);
  });
});
