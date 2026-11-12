import { describe, expect, it } from 'vitest';
import type { SidebarItem } from '../session/grouping';
import { filterItems, folderSpaces, openOnPick, spaceTrail, spaces, toggleOpen, visibleIds, type SpaceNode } from './spaces';
import { NO_WORKSPACE } from '../state.svelte';
import type { Folder, Workspace } from '../types';

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

  it('lists a workspace holding only a running shell as a row, without counting the shell', () => {
    // Given a workspace whose only item is a running shell
    // When the spaces are built
    // Then it is a row holding the shell, its summary counts no session, and it is not an empty chip
    const r = spaces([ws('a', 'A')], [it_('s', 'running', { kind: 'shell' })], new Map([['s', 'a']]), '');
    expect(r.rows.map((x) => x.items.map((i) => i.key))).toEqual([['s']]);
    expect(r.rows[0]!.summary.total).toBe(0);
    expect(r.empty).toEqual([]);
  });

  it('keeps a workspace holding only an exited shell empty', () => {
    // Given a workspace whose only item is an exited shell
    // When the spaces are built
    // Then it stays an empty chip
    const r = spaces([ws('a', 'A')], [it_('s', 'closed', { kind: 'shell' })], new Map([['s', 'a']]), '');
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

  it('visibleIds lists the workspaces a session can sit under', () => {
    // Given live, archived and ghost workspaces
    // When the visible ids are taken
    // Then only the live one remains
    const all = [ws('a', 'A'), ws('b', 'B', { archived: true }), ws('g', 'G', { auto: true, missing: true } as Partial<Workspace>)];
    expect([...visibleIds(all)]).toEqual(['a']);
  });

  it('filterItems keeps a session whose title or workspace name matches, and everything without a filter', () => {
    // Given two sessions in two workspaces
    // When filtering by a workspace name, a title, and nothing
    // Then the matches stay
    const items = [it_('k1', 'open', { title: 'Fix login' }), it_('k2', 'open', { title: 'Docs' })];
    const name = (key: string) => (key === 'k1' ? 'webshop' : 'infra');
    expect(filterItems(items, name, 'INFRA').map((i) => i.key)).toEqual(['k2']);
    expect(filterItems(items, name, 'login').map((i) => i.key)).toEqual(['k1']);
    expect(filterItems(items, name, '  ')).toBe(items);
  });
});

const fo = (id: string, name: string, parentId: string | null = null, over: Partial<Folder> = {}) => ({ id, name, parentId, rank: 1, collapsed: false, ...over }) as Folder;
// One token per node: kind, depth, then the folder name, the row id or the empty chips.
const shape = (nodes: SpaceNode[]) =>
  nodes.map((n) => (n.type === 'folder' ? `F${n.depth}:${n.folder?.name ?? '*'}` : n.type === 'space' ? `W${n.depth}:${n.row.id}` : `E${n.depth}:${n.workspaces.map((w) => w.id).join(',')}`));
const tree = (workspaces: Workspace[], folders: Folder[], keys: [string, string, SidebarItem['state']?][], filter = '') => {
  const items = keys.map(([k, , s]) => it_(k, s ?? 'open'));
  return folderSpaces(spaces(workspaces, items, new Map(keys.map(([k, w]) => [k, w])), filter), folders, filter);
};

describe('folderSpaces', () => {
  it('puts a top-level folder as a section and its workspaces one level in, root workspaces after', () => {
    // Given a workspace in folder E and one outside any folder
    // When the folder tree is built
    // Then E is a depth-0 section holding a at depth 1, and b follows at the root
    const r = tree([ws('a', 'A', { folderId: 'e' }), ws('b', 'B', { rank: 2, folderId: null })], [fo('e', 'E')], [['k1', 'a'], ['k2', 'b']]);
    expect(shape(r)).toEqual(['F0:E', 'W1:a', 'W0:b']);
  });

  it('nests folders without a depth limit, one level per parent', () => {
    // Given folders E > core > tenants > archives with a workspace at the bottom
    // When the folder tree is built
    // Then each level sits one deeper than its parent
    const folders = [fo('e', 'E'), fo('c', 'core', 'e'), fo('t', 'tenants', 'c'), fo('x', 'archives', 't')];
    const r = tree([ws('a', 'A', { folderId: 'x' })], folders, [['k1', 'a']]);
    expect(shape(r)).toEqual(['F0:E', 'F1:core', 'F2:tenants', 'F3:archives', 'W4:a']);
  });

  it('lists a folder\'s workspaces with sessions, then its subfolders, then its empty workspaces', () => {
    // Given a folder holding a busy workspace, an empty one and a subfolder
    // When the folder tree is built
    // Then the order is workspace, subfolder, no-session line, each at its level
    const folders = [fo('e', 'E'), fo('c', 'core', 'e')];
    const r = tree([ws('a', 'A', { folderId: 'e' }), ws('b', 'B', { folderId: 'e' }), ws('d', 'D', { folderId: 'c' })], folders, [['k1', 'a'], ['k2', 'd']]);
    expect(shape(r)).toEqual(['F0:E', 'W1:a', 'F1:core', 'W2:d', 'E1:b']);
  });

  it('hides the subtree of a collapsed folder and sums the states of all of it', () => {
    // Given a collapsed folder whose sessions sit in it and in a subfolder
    // When the folder tree is built
    // Then only the folder line is left, carrying the counts and total of the whole subtree
    const folders = [fo('e', 'E'), fo('f', 'fleet', 'e', { collapsed: true }), fo('g', 'deep', 'f')];
    const r = tree([ws('a', 'A', { folderId: 'f' }), ws('b', 'B', { folderId: 'g' })], folders, [['k1', 'a', 'attention'], ['k2', 'a', 'running'], ['k3', 'b', 'paused'], ['k4', 'b', 'closed']]);
    expect(shape(r)).toEqual(['F0:E', 'F1:fleet']);
    const fleet = r[1] as Extract<SpaceNode, { type: 'folder' }>;
    expect(fleet.open).toBe(false);
    expect(fleet.summary.counts).toMatchObject({ attention: 1, running: 1, paused: 1, closed: 1 });
    expect(fleet.summary.total).toBe(4);
    expect(fleet.summary.urgent).toBe('attention');
  });

  it('counts the direct subfolders and every workspace of the subtree for the section line', () => {
    // Given a section with two subfolders, one nested deeper, and four workspaces spread over them
    // When the folder tree is built
    // Then the section reports 2 folders and 4 workspaces, busy or empty
    const folders = [fo('e', 'E'), fo('c', 'core', 'e'), fo('l', 'labs', 'e', { rank: 2 }), fo('t', 'tenants', 'c')];
    const r = tree([ws('a', 'A', { folderId: 'e' }), ws('b', 'B', { folderId: 'c' }), ws('d', 'D', { folderId: 't' }), ws('z', 'Z', { folderId: 'l' })], folders, [['k1', 'a'], ['k2', 'd']]);
    const section = r[0] as Extract<SpaceNode, { type: 'folder' }>;
    expect(section).toMatchObject({ folders: 2, spaces: 4 });
  });

  it('keeps an empty folder visible without a filter', () => {
    // Given a folder with nothing in it
    // When the folder tree is built without a filter
    // Then its line is still there, with a zero total
    const r = tree([], [fo('e', 'E')], []);
    expect(shape(r)).toEqual(['F0:E']);
    expect((r[0] as Extract<SpaceNode, { type: 'folder' }>).summary.total).toBe(0);
  });

  it('moves pinned workspaces to a pinned section out of their folder', () => {
    // Given a pinned workspace filed in folder E
    // When the folder tree is built
    // Then it leads under a pinned section at depth 0 and E keeps the rest
    const r = tree([ws('a', 'A', { folderId: 'e' }), ws('p', 'P', { folderId: 'e', pinned: true }), ws('q', 'Q', { pinned: true })], [fo('e', 'E')], [['k1', 'a'], ['k2', 'p']]);
    expect(shape(r)).toEqual(['F0:*', 'W0:p', 'E0:q', 'F0:E', 'W1:a']);
  });

  it('keeps the sessions outside any workspace last, after the root empty workspaces line', () => {
    // Given a root workspace with a session, a root empty one and an unassigned session
    // When the folder tree is built
    // Then the root order is busy workspace, other row, then the empty chips
    const r = tree([ws('a', 'A'), ws('b', 'B', { rank: 2 })], [], [['k1', 'a'], ['k2', 'none']]);
    expect(shape(r)).toEqual(['W0:a', 'W0:', 'E0:b']);
  });

  it('drops the folders without a match and opens the collapsed ones while filtering', () => {
    // Given a collapsed folder holding a match and another folder without any
    // When the tree is filtered
    // Then the matching folder is open and the other one is gone
    const folders = [fo('e', 'E', null, { collapsed: true }), fo('o', 'O', null, { rank: 2 })];
    const r = tree([ws('a', 'login', { folderId: 'e' }), ws('b', 'B', { folderId: 'o' })], folders, [['k1', 'a'], ['k2', 'b']], 'log');
    expect(shape(r)).toEqual(['F0:E', 'W1:a']);
    expect((r[0] as Extract<SpaceNode, { type: 'folder' }>).open).toBe(true);
  });

  it('treats a folder with an unknown parent as a section and a workspace in an unknown folder as root', () => {
    // Given a folder whose parent is gone and a workspace whose folder is gone
    // When the folder tree is built
    // Then the folder is a section and the workspace sits at the root
    const r = tree([ws('a', 'A', { folderId: 'gone' }), ws('b', 'B', { folderId: 'c' })], [fo('c', 'core', 'missing')], [['k1', 'a'], ['k2', 'b']]);
    expect(shape(r)).toEqual(['F0:core', 'W1:b', 'W0:a']);
  });

  it('survives a parent cycle without looping', () => {
    // Given two folders that are each other's parent
    // When the folder tree is built
    // Then it returns, and their workspace still shows once at the root
    const r = tree([ws('a', 'A', { folderId: 'x' })], [fo('x', 'X', 'y'), fo('y', 'Y', 'x')], [['k1', 'a']]);
    expect(shape(r).filter((s) => s === 'W0:a')).toHaveLength(1);
  });
});

describe('spaceTrail', () => {
  it('joins the folder chain and the workspace name', () => {
    // Given a workspace in E > core
    // When its trail is asked
    // Then it reads E › core › name
    const trail = spaceTrail({ workspaces: [ws('a', 'endurance-core', { folderId: 'c' })], folders: [fo('e', 'Endurance'), fo('c', 'core', 'e')], groups: [] });
    expect(trail('a')).toBe('Endurance › core › endurance-core');
  });

  it('is just the name outside folders, nothing for an unknown workspace, and stops on a cycle', () => {
    // Given a root workspace, an unknown id and a workspace under a folder cycle
    // When their trails are asked
    // Then the name, undefined and a finite trail come back
    const trail = spaceTrail({ workspaces: [ws('a', 'A'), ws('b', 'B', { folderId: 'x' })], folders: [fo('x', 'X', 'y'), fo('y', 'Y', 'x')], groups: [] });
    expect(trail('a')).toBe('A');
    expect(trail(null)).toBeUndefined();
    expect(trail('nope')).toBeUndefined();
    expect(trail('b')).toBe('Y › X › B');
  });
});

describe('spaces with archived workspaces shown', () => {
  it('lists an archived workspace with its sessions and flags, instead of dropping them to the other row', () => {
    // Given an archived workspace holding a session
    const items = [it_('k1', 'open')];
    const wsOf = new Map([['k1', 'a']]);
    const list = [ws('a', 'A', { archived: true })];
    // When the archive is shown, and when it is not
    const shown = spaces(list, items, wsOf, '', true);
    const hidden = spaces(list, items, wsOf, '');
    // Then the workspace owns its row only when shown
    expect(shown.rows.map((r) => r.id)).toEqual(['a']);
    expect(shown.rows[0]!.workspace?.archived).toBe(true);
    expect(hidden.rows.map((r) => r.id)).toEqual(['']);
  });

  it('lists an archived workspace without session among the empty ones, and keeps ghosts out', () => {
    // Given an archived workspace with no session and a ghost
    const list = [ws('a', 'A', { archived: true }), ws('g', 'G', { auto: true, missing: true } as Partial<Workspace>)];
    // When the archive is shown
    const shown = spaces(list, [], new Map(), '', true);
    // Then only the archived one is listed
    expect(shown.empty.map((x) => x.id)).toEqual(['a']);
    expect(spaces(list, [], new Map(), '').empty).toEqual([]);
  });
});

describe('openOnPick', () => {
  it('keeps the previously open workspace open when an unset list switches to another one', () => {
    // Given no saved list, workspace a active (so the only open one)
    // When b is picked
    // Then both stay open, instead of a folding away
    expect(openOnPick(undefined, 'a', 'b')).toEqual(['a', 'b']);
  });

  it('adds a closed workspace to the saved list and changes nothing for an open one', () => {
    // Given a saved list holding a
    // When b is picked, then a
    // Then only b needs a write
    expect(openOnPick(['a'], 'a', 'b')).toEqual(['a', 'b']);
    expect(openOnPick(['a'], 'b', 'a')).toBeNull();
    expect(openOnPick(undefined, 'a', 'a')).toBeNull();
  });
});

describe('spaces filter on paths', () => {
  it('keeps a workspace whose path or cwd matches, with every session, and lists an empty one by path', () => {
    // Given a workspace matched by its path, one by its cwd, and an empty one by path
    const list = [ws('a', 'A', { path: '/src/billing' } as Partial<Workspace>), ws('b', 'B', { cwd: '/src/billing-b' } as Partial<Workspace>), ws('c', 'C', { path: '/src/billing-c' } as Partial<Workspace>)];
    const items = [it_('k1', 'open', { title: 'x' }), it_('k2', 'open', { title: 'y' })];
    // When filtering on "billing"
    const r = spaces(list, items, new Map([['k1', 'a'], ['k2', 'b']]), 'billing');
    // Then both rows keep their session and the empty workspace is listed
    expect(r.rows.map((x) => [x.id, x.items.length])).toEqual([['a', 1], ['b', 1]]);
    expect(r.empty.map((x) => x.id)).toEqual(['c']);
  });

  it('numbers the unassigned row like NO_WORKSPACE so picking it selects that space', () => {
    // Given a session outside any workspace
    // When the rows are built
    // Then the row id is the id the app uses for "no workspace"
    const r = spaces([], [it_('k1', 'open')], new Map(), '');
    expect(r.rows[0]!.id).toBe(NO_WORKSPACE);
  });
});


describe('spaces with nested workspaces', () => {
  const parent = ws('p', 'endurance', { path: '/e' });
  const onboarding = ws('n1', 'Onboarding', { path: '/e/20/onboarding', parentId: 'p', rank: 2 });
  const os = ws('n2', 'endurance-os', { path: '/e/40/os', parentId: 'p', auto: true, rank: 1 });
  const all = [parent, onboarding, os, ws('o', 'other', { rank: 3 })];

  it('lists nested workspaces inside their parent row, never at the top level, and sums their sessions into it', () => {
    // Given a session at the parent root and two in a nested workspace
    const items = [it_('root', 'paused'), it_('a', 'attention'), it_('b', 'running')];
    const wsOf = new Map([['root', 'p'], ['a', 'n1'], ['b', 'n1']]);
    // When the spaces are built
    const r = spaces(all, items, wsOf, '');
    // Then the parent row holds its own session, both nested workspaces in rank order, and the totals of all three
    expect(r.rows.map((x) => x.id)).toEqual(['p']);
    const row = r.rows[0]!;
    expect(row.items.map((i) => i.key)).toEqual(['root']);
    expect(row.children!.map((c) => [c.id, c.items.length])).toEqual([['n2', 0], ['n1', 2]]);
    expect(row.summary.total).toBe(3);
    expect(row.summary.urgent).toBe('attention');
    expect(r.empty.map((w) => w.id)).toEqual(['o']);
  });

  it('shows the parent block when only a nested workspace has sessions', () => {
    const r = spaces(all, [it_('a', 'open')], new Map([['a', 'n1']]), '');
    expect(r.rows[0]!.id).toBe('p');
    expect(r.rows[0]!.items).toEqual([]);
    expect(r.rows[0]!.children!.find((c) => c.id === 'n1')!.items).toHaveLength(1);
  });

  it('keeps an idle parent as a block listing its nested workspaces, so they can be started in', () => {
    const r = spaces(all, [], new Map(), '');
    expect(r.rows.map((x) => x.id)).toEqual(['p']);
    expect(r.rows[0]!.children!.map((c) => c.id)).toEqual(['n2', 'n1']);
    expect(r.rows[0]!.summary.total).toBe(0);
    expect(r.empty.map((w) => w.id)).toEqual(['o']);
  });

  it('drops an idle parent whose name and nested workspaces miss the filter', () => {
    const r = spaces(all, [], new Map(), 'zzz');
    expect(r.rows).toEqual([]);
    expect(r.empty).toEqual([]);
  });

  it('hides nested workspaces with an archived parent and sends their sessions to "other"', () => {
    const r = spaces([{ ...parent, archived: true }, onboarding], [it_('a', 'open')], new Map([['a', 'n1']]), '');
    expect(r.rows.map((x) => x.id)).toEqual(['']);
  });

  it('keeps on a filter only the nested workspaces that match, by name or session title', () => {
    const items = [it_('fix kyc', 'open'), it_('image', 'open')];
    const wsOf = new Map([['fix kyc', 'n1'], ['image', 'n2']]);
    const byTitle = spaces(all, items, wsOf, 'kyc');
    expect(byTitle.rows[0]!.children!.map((c) => c.id)).toEqual(['n1']);
    const byName = spaces(all, items, wsOf, 'endurance-os');
    expect(byName.rows[0]!.children!.map((c) => [c.id, c.items.length])).toEqual([['n2', 1]]);
    const byParent = spaces(all, items, wsOf, 'endurance');
    expect(byParent.rows[0]!.children!.map((c) => c.items.length)).toEqual([1, 1]);
  });

  it('builds the parent row in linear work for many nested workspaces', () => {
    const many = Array.from({ length: 400 }, (_, i) => ws(`k${i}`, `kid ${i}`, { path: `/e/k${i}`, parentId: 'p', rank: i }));
    const items = many.map((k) => it_(`s${k.id}`, 'open'));
    const wsOf = new Map(many.map((k) => [`s${k.id}`, k.id]));
    const r = spaces([parent, ...many], items, wsOf, '');
    expect(r.rows[0]!.children).toHaveLength(400);
    expect(r.rows[0]!.summary.total).toBe(400);
  });
});

describe('spaceTrail of a nested workspace', () => {
  it('reads the parent folder chain, the parent, then the nested name', () => {
    const trail = spaceTrail({
      workspaces: [ws('p', 'endurance', { folderId: 'c' }), ws('n', 'Onboarding PME', { parentId: 'p' }), ws('lost', 'Lost', { parentId: 'gone' })],
      folders: [fo('c', 'Clients')],
      groups: [],
    });
    expect(trail('n')).toBe('Clients › endurance › Onboarding PME');
    expect(trail('lost')).toBe('Lost');
  });
});
