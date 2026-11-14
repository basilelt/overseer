import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { SidebarItem } from '../../session/grouping';
import { spaces } from '../../workspace/spaces';
import type { Workspace } from '../../types';
import SpaceBlock from './SpaceBlock.svelte';

const NOW = Date.parse('2026-11-05T10:05:00Z');
const w = { id: 'a', name: 'webshop', rank: 1, pinned: false, archived: false } as Workspace;
const item = (key: string, state: SidebarItem['state'], at = '2026-11-05T10:00:00Z') => ({ key, kind: 'claude', group: 'ready', state, title: `T-${key}`, at }) as SidebarItem;
const row = (items: SidebarItem[]) => spaces([w], items, new Map(items.map((i) => [i.key, 'a'])), '').rows[0]!;
const html = (items: SidebarItem[], extra: Record<string, unknown> = {}) =>
  render(SpaceBlock, { props: { row: row(items), now: NOW, open: false, closedOpen: false, selectedKey: null, ontoggle: () => {}, onclosed: () => {}, onselect: () => {}, onpick: () => {}, ...extra } }).body;

describe('SpaceBlock', () => {
  it('collapsed: one line with name, a pill per non-zero live state, and the total', () => {
    const h = html([item('1', 'attention'), item('2', 'running'), item('3', 'open')]);
    expect(h).toContain('>webshop<');
    expect(h).toContain('data-pill="attention"');
    expect(h).toContain('data-pill="running"');
    expect(h).not.toContain('data-pill="paused"');
    expect(h).toContain('class="total');
    expect(h).not.toContain('T-1');
    expect(h).toContain('aria-expanded="false"');
  });

  it('expanded: a frame holds the sessions sorted by urgency, no subtitle', () => {
    const h = html([item('o', 'open'), item('r', 'running'), item('a', 'attention')], { open: true });
    expect(h).toContain('aria-expanded="true"');
    const at = ['T-a', 'T-r', 'T-o'].map((s) => h.indexOf(s));
    expect(at.every((i) => i > 0)).toBe(true);
    expect([...at].sort((x, y) => x - y)).toEqual(at);
  });

  it('shows "+ N closed" only when some are closed, and reveals them by time bucket on demand', () => {
    const items = [item('l', 'open'), item('c1', 'closed'), item('c2', 'closed', '2026-11-03T10:00:00Z')];
    const shut = html(items, { open: true });
    expect(shut).toContain('+ 2 closed');
    expect(shut).not.toContain('T-c1');
    const opened = html(items, { open: true, closedOpen: true });
    expect(opened).toContain('T-c1');
    expect(opened).toContain('T-c2');
    expect(opened).toContain('Today');
    expect(html([item('l', 'open')], { open: true })).not.toContain('closed');
  });

  it('lists running shells discreetly after the live sessions and before the closed count', () => {
    const h = html([item('l', 'open'), { ...item('s', 'running'), kind: 'shell' }, item('c', 'closed')], { open: true });
    const [l, s, c] = ['T-l', 'T-s', '+ 1 closed'].map((x) => h.indexOf(x));
    expect(l).toBeGreaterThan(0);
    expect(s).toBeGreaterThan(l);
    expect(c).toBeGreaterThan(s);
    expect(h).toMatch(/class="row[^"]*\bshell\b/);
  });

  it('expands a workspace holding only a shell into a frame with that shell', () => {
    expect(html([{ ...item('s', 'running'), kind: 'shell' }], { open: true })).toContain('T-s');
  });

  it('splits the header: the name picks the workspace, a separate chevron folds it', () => {
    const h = html([item('1', 'open')]);
    expect(h.match(/<button/g)).toHaveLength(2);
    expect(h.match(/aria-expanded/g)).toHaveLength(1);
    expect(h).toMatch(/<button[^>]*class="chev[^"]*"[^>]*aria-expanded="false"|<button[^>]*aria-expanded="false"[^>]*class="chev/);
  });

  it('marks the selected workspace, the tab-drop target and the archived one', () => {
    expect(html([item('1', 'open')], { selected: true })).toContain('selected');
    expect(html([item('1', 'open')], { selected: false })).not.toMatch(/class="[^"]*selected/);
    expect(html([item('1', 'open')], { tabDrop: true })).toMatch(/class="[^"]*tab-drop/);
    const archived = render(SpaceBlock, { props: { row: { ...row([item('1', 'open')]), workspace: { ...w, archived: true } }, now: NOW, open: false, closedOpen: false, selectedKey: null, ontoggle: () => {}, onclosed: () => {}, onselect: () => {}, onpick: () => {} } }).body;
    expect(archived).toMatch(/class="[^"]*archived/);
  });

  it('exposes the header as a tab-drop and drag-and-drop target of its workspace, but not for "Other"', () => {
    const h = html([item('1', 'open')], { dropPos: 'before' });
    expect(h).toContain('data-workspace-id="a"');
    expect(h).toContain('data-drop="workspace"');
    expect(h).toContain('data-id="a"');
    expect(h).toMatch(/class="[^"]*drop-before/);
    const other = render(SpaceBlock, { props: { row: { id: '', name: null, workspace: null, items: [item('1', 'open')], summary: row([item('1', 'open')]).summary }, now: NOW, open: false, closedOpen: false, selectedKey: null, ontoggle: () => {}, onclosed: () => {}, onselect: () => {}, onpick: () => {} } }).body;
    expect(other).not.toContain('data-workspace-id');
    expect(other).not.toContain('data-drop');
  });

  it('words the closed count in singular and plural', () => {
    expect(html([item('l', 'open'), item('c', 'closed')], { open: true })).toContain('+ 1 closed');
    expect(html([item('l', 'open'), item('c', 'closed'), item('d', 'closed')], { open: true })).toContain('+ 2 closed');
  });

  it('flags a workspace whose folder is gone, with its path as the tooltip', () => {
    const missing = { ...w, missing: true, path: '/src/gone' } as Workspace;
    const h = render(SpaceBlock, { props: { row: { ...row([item('1', 'open')]), workspace: missing }, now: NOW, open: false, closedOpen: false, selectedKey: null, ontoggle: () => {}, onclosed: () => {}, onselect: () => {}, onpick: () => {} } }).body;
    expect(h).toMatch(/class="[^"]*missing/);
    expect(h).toContain('Folder missing');
    expect(h).toContain('title="/src/gone"');
    expect(html([item('1', 'open')])).not.toContain('Folder missing');
  });

  it('labels the chevron as a fold action, apart from the name button', () => {
    expect(html([item('1', 'open')])).toContain('aria-label="Expand webshop"');
    expect(html([item('1', 'open')], { open: true })).toContain('aria-label="Collapse webshop"');
  });
});

describe('SpaceBlock multi-select', () => {
  it('hands the multi selection to every session row, live, shells and closed', () => {
    const items = [item('l', 'open'), { ...item('s', 'running'), kind: 'shell' as const }, item('c', 'closed')];
    const rows = (multi: Set<string>) => html(items, { open: true, closedOpen: true, multi }).match(/class="row[^"]*\bmulti\b/g)?.length ?? 0;
    expect(rows(new Set(['l', 's', 'c']))).toBe(3);
    expect(rows(new Set(['l']))).toBe(1);
    expect(rows(new Set())).toBe(0);
  });

  describe('nested workspaces', () => {
    const parent = { id: 'p', name: 'endurance', path: '/e', rank: 1, pinned: false, archived: false } as Workspace;
    const folder = { id: 'n1', name: 'Onboarding', path: '/e/20/onboarding', parentId: 'p', rank: 1, pinned: false, archived: false, auto: false } as Workspace;
    const sub = { id: 'n2', name: 'endurance-os', path: '/e/40/os', parentId: 'p', rank: 2, pinned: false, archived: false, auto: true } as Workspace;
    const subject = { id: 'n3', name: 'Archi', path: null, kind: 'subject', cwd: '/e', parentId: 'p', rank: 3, pinned: false, archived: false } as Workspace;
    const items = [item('root', 'open'), item('kyc', 'attention')];
    const nestedRow = spaces([parent, folder, sub, subject], items, new Map([['root', 'p'], ['kyc', 'n1']]), '').rows[0]!;
    const kids = (over: Record<string, unknown> = {}) => ({
      isOpen: (id: string) => id === 'n1',
      isClosedOpen: () => false,
      selectedId: 'n1',
      tabDrop: null,
      sessionDrop: 'n2',
      ontoggle: () => {},
      onpick: () => {},
      onclosed: () => {},
      onmenu: () => {},
      onstart: () => {},
      onnew: () => {},
      ...over,
    });
    const nested = (extra: Record<string, unknown> = {}) =>
      render(SpaceBlock, { props: { row: nestedRow, now: NOW, open: true, closedOpen: false, selectedKey: null, kids: kids(), ontoggle: () => {}, onclosed: () => {}, onselect: () => {}, onpick: () => {}, ...extra } }).body;

    it('lists the root sessions first, then each nested workspace line with its kind, drop zone and start button', () => {
      const h = nested();
      expect(h.indexOf('T-root')).toBeLessThan(h.indexOf('data-id="n1"'));
      for (const [id, kind] of [['n1', 'folder'], ['n2', 'submodule'], ['n3', 'subject']]) {
        expect(h).toMatch(new RegExp(`data-workspace-id="${id}"[^>]*data-drop="child"[^>]*data-id="${id}"[^>]*data-kind="${kind}"`));
      }
      expect(h.match(/class="start/g)).toHaveLength(3);
      expect(h).toContain('title="/e/20/onboarding"');
    });

    it('shows the sessions of an open nested workspace and marks the selected one and the session drop target', () => {
      const h = nested();
      expect(h.indexOf('T-kyc')).toBeGreaterThan(h.indexOf('data-id="n1"'));
      expect(h.indexOf('T-kyc')).toBeLessThan(h.indexOf('data-id="n2"'));
      expect(h).toMatch(/class="line child[^"]*selected[^"]*"[^>]*data-workspace-id="n1"/);
      expect(h).toMatch(/class="line child[^"]*tab-drop[^"]*"[^>]*data-workspace-id="n2"/);
      expect(h).toMatch(/class="line child[^"]*idle[^"]*"[^>]*data-workspace-id="n2"/);
    });

    it('offers "+ New subject" only with a handler, and nothing nested without hooks or when folded', () => {
      expect(nested()).toContain('+ New subject');
      expect(nested({ kids: kids({ onnew: undefined }) })).not.toContain('+ New subject');
      expect(nested({ kids: undefined })).not.toContain('data-drop="child"');
      expect(nested({ open: false })).not.toContain('data-drop="child"');
    });

    it('puts the summed pills of the nested sessions on the parent line', () => {
      const h = nested({ open: false });
      expect(h).toContain('data-pill="attention"');
      expect(h).toMatch(/class="total[^"]*">2</);
    });
  });
});
