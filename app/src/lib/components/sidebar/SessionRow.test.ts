import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { SidebarItem } from '../../grouping';
import SessionRow from './SessionRow.svelte';
import componentSrc from './SessionRow.svelte?raw';

const NOW = Date.parse('2026-11-05T10:05:00Z');
const item = (over: Partial<SidebarItem> = {}): SidebarItem => ({ key: 'line:a', kind: 'claude', group: 'running', state: 'running', title: 'Fix login', status: 'Ran the tests', at: '2026-11-05T10:00:00Z', ...over });
const html = (over: Partial<SidebarItem> = {}, extra: Record<string, unknown> = {}) => render(SessionRow, { props: { item: item(over), now: NOW, onselect: () => {}, ...extra } }).body;

describe('SessionRow', () => {
  it('is one native button, reachable by keyboard, so Enter and Space open the session', () => {
    // Given a session row
    // When it renders
    // Then it is a focusable button that is not taken out of the tab order
    const h = html();
    expect(h).toMatch(/^(<!--\[-->)?<button/);
    expect(h).toContain('type="button"');
    expect(h).not.toContain('tabindex="-1"');
  });

  it('keeps its content readable and adds the state for screen readers', () => {
    // Given a paused session
    // When it renders
    // Then no aria-label hides the content and the state label is in a sr-only span
    const h = html({ state: 'paused' });
    expect(h).not.toContain('aria-label');
    expect(h).toContain('class="sr-only">Paused<');
    for (const s of ['>Fix login<', '>Ran the tests<', '>5m<']) expect(h).toContain(s);
  });

  it('opens the session on click', () => {
    // Given the component source
    // When the click handler is read
    // Then it forwards the event to onselect
    expect(componentSrc).toMatch(/onclick=\{\(e\) => onselect\(e\)\}/);
  });


  it('shows title, last action, age and chevron in that order', () => {
    // Given a session with a last action
    // When it renders
    // Then title, action, age and chevron appear left to right
    const h = html();
    const at = ['>Fix login<', '>Ran the tests<', '>5m<', '›'].map((s) => h.indexOf(s));
    expect(at.every((i) => i >= 0)).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
  });

  it('omits the last action when there is none', () => {
    // Given a session without a status
    // When it renders
    // Then no action element is drawn
    expect(html({ status: undefined })).not.toContain('class="action');
  });

  it('draws a filled dot per state through the state token', () => {
    // Given each live state
    // When it renders
    // Then the dot carries the state and its colour token
    for (const s of ['running', 'attention', 'open'] as const) {
      const h = html({ state: s });
      expect(h).toContain(`data-state="${s}"`);
      expect(h).toContain(`--c: var(--state-${s})`);
      expect(h).not.toContain('data-hollow');
    }
  });

  it('shows the pause glyph on a paused session', () => {
    // Given a paused session
    // When it renders
    // Then the dot holds the ❙❙ glyph
    expect(html({ state: 'paused' })).toContain('❙❙');
    expect(html({ state: 'running' })).not.toContain('❙❙');
  });

  it('draws a hollow dot on a closed session', () => {
    // Given a closed session
    // When it renders
    // Then the dot is marked hollow
    expect(html({ state: 'closed' })).toContain('data-hollow');
  });

  it('does not repeat the host on every row: the title bar and host sections already name it', () => {
    // Given a session on an SSH host
    // When it renders, even if a caller still passes a host
    // Then the row carries no host name nor connection status
    const h = html({}, { host: { name: 'devbox', phase: 'connected' } });
    expect(h).not.toContain('devbox');
    expect(h).not.toContain('Connected');
    expect(componentSrc).not.toContain('class="host"');
  });

  it('uses a dedicated, readable selection token and a text-2 age', () => {
    // Given the component source
    // When its styles are read
    // Then selection has its own token and the age is not text-3
    expect(componentSrc).toContain('var(--session-row-selected)');
    expect(componentSrc).not.toContain('--text-3');
  });

  it('marks the selected row', () => {
    // Given a selected row
    // When it renders
    // Then aria-current is set
    expect(html({}, { selected: true })).toContain('aria-current="true"');
    expect(html()).not.toContain('aria-current');
  });

  it('never colours a border by state and has no transition', () => {
    // Given the component source
    // When its styles are read
    // Then the state colour is only used for the dot and nothing animates
    const src = componentSrc;
    expect(src).not.toMatch(/border(-color)?:[^;]*(--c|--state)/);
    expect(src).not.toMatch(/transition|animation/);
  });

  it('gives the title priority: the last action shrinks first', () => {
    // Given the component source
    // When the flex rules are read
    // Then the title keeps a minimum width and the action takes the leftover
    expect(componentSrc).toMatch(/\.title \{[^}]*flex: 0 1 auto[^}]*min-width: \d+px/s);
    expect(componentSrc).toMatch(/\.action \{[^}]*flex: 1 1 0[^}]*min-width: 0/s);
  });


  it('keeps the chevron in view: the row clips and the title has a minimum', () => {
    // Given the component source
    // When the layout rules are read
    // Then the row clips, the title has a px minimum and the chevron never shrinks
    const rule = (sel: string) => componentSrc.match(new RegExp(`\\${sel} \\{([^}]*)\\}`))![1]!;
    expect(rule('.row')).toContain('overflow: hidden');
    expect(rule('.title')).toMatch(/min-width: \d+px/);
    expect(rule('.chev')).toContain('flex: none');
  });
});

describe('SessionRow multi-select', () => {
  it('marks a multi-selected row with the multi class, and only then', () => {
    // Given a row in the multi selection and one outside it
    // When they render
    // Then only the first carries the multi class
    expect(html({}, { multi: true })).toMatch(/class="[^"]*\bmulti\b/);
    expect(html({}, { multi: false })).not.toMatch(/class="[^"]*\bmulti\b/);
  });

  it('styles the multi row from the accent token so it reads in every theme', () => {
    // Given the component source
    // When the multi rule is read
    // Then it mixes the accent into the row background and rings it with the accent, no hard-coded colour
    const rule = componentSrc.match(/\.row\.multi \{([^}]*)\}/)![1]!;
    expect(rule).toContain('var(--accent)');
    expect(rule).toContain('var(--session-row-bg)');
    expect(rule).not.toMatch(/#[0-9a-f]{3,6}/i);
  });
});
