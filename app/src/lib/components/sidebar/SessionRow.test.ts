import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { SidebarItem } from '../../session/grouping';
import SessionRow from './SessionRow.svelte';
import componentSrc from './SessionRow.svelte?raw';
import { declOf, parseStyle } from '../../../test/css';

const NOW = Date.parse('2026-11-05T10:05:00Z');
const item = (over: Partial<SidebarItem> = {}): SidebarItem => ({ key: 'line:a', kind: 'claude', group: 'running', state: 'running', title: 'Fix login', status: 'Ran the tests', at: '2026-11-05T10:00:00Z', ...over });
const html = (over: Partial<SidebarItem> = {}, extra: Record<string, unknown> = {}) => render(SessionRow, { props: { item: item(over), now: NOW, onselect: () => {}, ...extra } }).body;

describe('SessionRow', () => {
  it('is one native button, reachable by keyboard, so Enter and Space open the session', () => {
    const h = html();
    expect(h).toMatch(/^(<!--\[-->)?<div class="wrap[^"]*"><button class="row/);
    expect(h).toContain('type="button"');
    expect(h).not.toContain('tabindex="-1"');
  });

  it('keeps its content readable and adds the state for screen readers', () => {
    const h = html({ state: 'paused' });
    expect(h).not.toContain('aria-label');
    expect(h).toContain('class="sr-only">Paused<');
    for (const s of ['>Fix login<', '>Ran the tests<', '>5m<']) expect(h).toContain(s);
  });

  it('draws a plain shell muted, with a terminal icon instead of the state dot', () => {
    const sh = html({ kind: 'shell', status: undefined, title: 'zsh' });
    expect(sh).toMatch(/class="row[^"]*\bshell\b/);
    expect(sh).toContain('class="icon');
    expect(sh).not.toContain('class="dot');
    expect(sh).toContain('>zsh<');
    expect(html()).toContain('class="dot');
    expect(html()).not.toMatch(/class="row[^"]*\bshell\b/);
  });

  it('gives an agent session a sticky note button next to the row, not inside it', () => {
    const empty = html({ lineId: 'ln_1' });
    const between = empty.slice(empty.indexOf('class="chev'), empty.indexOf('class="note'));
    expect(between).toContain('</button>');
    expect(empty).toContain('data-notes="ln_1"');
    expect(empty).toContain('aria-label="Add a sticky note"');
    expect(empty).not.toMatch(/class="wrap[^"]*\bnoted\b/);
    const two = html({ lineId: 'ln_1', notes: 2 });
    expect(two).toMatch(/class="wrap[^"]*\bnoted\b/);
    expect(two).toContain('aria-label="2 sticky notes"');
    expect(html({ lineId: 'ln_1', notes: 1 })).toContain('aria-label="1 sticky note"');
    expect(html({ key: 'term:t', kind: 'shell', termId: 't' })).not.toContain('data-notes');
  });

  it('shows title, last action, age and chevron in that order', () => {
    const h = html();
    const at = ['>Fix login<', '>Ran the tests<', '>5m<', '›'].map((s) => h.indexOf(s));
    expect(at.every((i) => i >= 0)).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
  });

  it('omits the last action when there is none', () => {
    expect(html({ status: undefined })).not.toContain('class="action');
  });

  it('draws a filled dot per state through the state token', () => {
    for (const s of ['running', 'attention', 'open'] as const) {
      const h = html({ state: s });
      expect(h).toContain(`data-state="${s}"`);
      expect(h).toContain(`--c: var(--state-${s})`);
      expect(h).not.toContain('data-hollow');
    }
  });

  it('shows the pause glyph on a paused session', () => {
    expect(html({ state: 'paused' })).toContain('❙❙');
    expect(html({ state: 'running' })).not.toContain('❙❙');
  });

  it('draws a hollow dot on a closed session', () => {
    expect(html({ state: 'closed' })).toContain('data-hollow');
  });

  it('does not repeat the host on every row: the title bar and host sections already name it', () => {
    const h = html({}, { host: { name: 'devbox', phase: 'connected' } });
    expect(h).not.toContain('devbox');
    expect(h).not.toContain('Connected');
  });

  it('marks the selected row', () => {
    expect(html({}, { selected: true })).toContain('aria-current="true"');
    expect(html()).not.toContain('aria-current');
  });

  it('CSS guard: the selected row and the age use the readable theme tokens', () => {
    expect(declOf(componentSrc, ".row[aria-current='true']", 'background')).toBe('var(--session-row-selected)');
    expect(declOf(componentSrc, '.age', 'color')).toBe('var(--text-2)');
  });

  it('CSS guard: shows the note button at once on touch screens, which have no hover', () => {
    expect(declOf(componentSrc, '.wrap.agent .note', 'opacity', '@media (hover: none)')).toBe('1');
    expect(declOf(componentSrc, '.note', 'opacity')).toBe('0');
  });

  it('CSS guard: gives the title priority, so the last action shrinks first', () => {
    expect(declOf(componentSrc, '.title', 'flex')).toBe('0 1 auto');
    expect(declOf(componentSrc, '.title', 'min-width')).toMatch(/^\d+px$/);
    expect(declOf(componentSrc, '.action', 'flex')).toBe('1 1 0');
    expect(declOf(componentSrc, '.action', 'min-width')).toBe('0');
  });

  it('CSS guard: keeps the chevron in view, as the row clips and the title has a minimum', () => {
    expect(declOf(componentSrc, '.row', 'overflow')).toBe('hidden');
    expect(declOf(componentSrc, '.chev', 'flex')).toBe('none');
  });

  it('CSS guard: no rule animates the row', () => {
    for (const r of parseStyle(componentSrc)) expect(Object.keys(r.decls).filter((k) => /^(transition|animation)/.test(k))).toEqual([]);
  });
});

describe('SessionRow multi-select', () => {
  it('marks a multi-selected row with the multi class, and only then', () => {
    expect(html({}, { multi: true })).toMatch(/class="[^"]*\bmulti\b/);
    expect(html({}, { multi: false })).not.toMatch(/class="[^"]*\bmulti\b/);
  });

  it('CSS guard: styles the multi row from the accent token, never a literal colour, so it reads in every theme', () => {
    expect(declOf(componentSrc, '.row.multi', 'background')).toContain('var(--accent)');
    expect(declOf(componentSrc, '.row.multi', 'background')).toContain('var(--session-row-bg)');
    expect(declOf(componentSrc, '.row.multi', 'outline')).toContain('var(--accent)');
  });
});
