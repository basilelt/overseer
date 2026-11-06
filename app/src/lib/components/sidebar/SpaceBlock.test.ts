import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { SidebarItem } from '../../grouping';
import { spaces } from '../../spaces';
import type { Workspace } from '../../types';
import SpaceBlock from './SpaceBlock.svelte';
import componentSrc from './SpaceBlock.svelte?raw';

const NOW = Date.parse('2026-11-05T10:05:00Z');
const w = { id: 'a', name: 'webshop', rank: 1, pinned: false, archived: false } as Workspace;
const item = (key: string, state: SidebarItem['state'], at = '2026-11-05T10:00:00Z') => ({ key, kind: 'claude', group: 'ready', state, title: `T-${key}`, at }) as SidebarItem;
const row = (items: SidebarItem[]) => spaces([w], items, new Map(items.map((i) => [i.key, 'a'])), '').rows[0]!;
const html = (items: SidebarItem[], extra: Record<string, unknown> = {}) =>
  render(SpaceBlock, { props: { row: row(items), now: NOW, open: false, closedOpen: false, selectedKey: null, ontoggle: () => {}, onclosed: () => {}, onselect: () => {}, ...extra } }).body;

describe('SpaceBlock', () => {
  it('collapsed: one line with name, a pill per non-zero live state, and the total', () => {
    // Given attention, running and open sessions, none paused
    // When it renders collapsed
    // Then orange and green pills show, no violet one, and no session rows
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
    // Given open, running and attention sessions
    // When it renders expanded
    // Then rows come attention, running, open
    const h = html([item('o', 'open'), item('r', 'running'), item('a', 'attention')], { open: true });
    expect(h).toContain('aria-expanded="true"');
    const at = ['T-a', 'T-r', 'T-o'].map((s) => h.indexOf(s));
    expect(at.every((i) => i > 0)).toBe(true);
    expect([...at].sort((x, y) => x - y)).toEqual(at);
  });

  it('shows "+ N closed" only when some are closed, and reveals them by time bucket on demand', () => {
    // Given one live and two closed sessions
    // When it renders expanded, then with the closed list open
    // Then the count line shows, and the closed rows only appear once opened
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

  it('passes the host to SSH rows only', () => {
    // Given an SSH host
    // When rows render
    // Then the chip shows
    expect(html([item('l', 'open')], { open: true, host: { name: 'devbox', phase: 'connected' } })).toContain('>devbox<');
    expect(html([item('l', 'open')], { open: true })).not.toContain('class="host');
  });

  it('forwards the row click and the toggle to the parent', () => {
    // Given the component source
    // When the handlers are read
    // Then rows call onselect with the key and the line calls ontoggle
    expect(componentSrc).toMatch(/onselect=\{\(\) => onselect\(it\.key\)\}|onselect=\{\(\) => onselect\(i\.key\)\}/);
    expect(componentSrc).toMatch(/onclick=\{ontoggle\}/);
    expect(componentSrc).not.toMatch(/transition|animation/);
  });
});
