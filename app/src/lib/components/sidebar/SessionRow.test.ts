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

  it('reads title, state label and age in its aria-label', () => {
    // Given a paused session
    // When it renders
    // Then the label names the title and the state
    expect(html({ state: 'paused' })).toContain('aria-label="Fix login, Paused"');
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

  it('shows the host chip only for an SSH session, with its connection status', () => {
    // Given a session on an SSH host and a local one
    // When they render
    // Then only the SSH one carries the host name and status
    const h = html({}, { host: { name: 'devbox', phase: 'connected' } });
    expect(h).toContain('>devbox<');
    expect(h).toContain('Connected');
    expect(html()).not.toContain('class="host');
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
});
