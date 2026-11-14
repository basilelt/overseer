import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import type { MenuItem } from '../../commands/menu';
import Menu from './Menu.svelte';

beforeEach(() => {
  vi.stubGlobal('innerWidth', 1200);
  vi.stubGlobal('innerHeight', 800);
});
afterEach(() => vi.unstubAllGlobals());

const html = (items: MenuItem[]) => render(Menu, { props: { items, x: 10, y: 10, label: 'Start', onclose: () => {} } }).body;

describe('menu groups', () => {
  const items: MenuItem[] = [{ label: 'Claude' }, { label: 'Not installed (2)', more: [{ label: 'Aider' }, { label: 'Goose' }] }];

  it('shows a group as one folded row that does not list its members', () => {
    const h = html(items);
    expect(h).toContain('role="menu"');
    expect(h).toContain('aria-label="Start"');
    expect(h).toMatch(/<button[^>]*aria-expanded="false"[^>]*>(?:(?!<\/button>)[\s\S])*Not installed \(2\)/);
    expect(h).not.toContain('Aider');
    expect(h).not.toContain('Goose');
  });

  it('gives plain entries no expand state, and marks a disabled one', () => {
    const h = html([{ label: 'Claude' }, { label: 'Codex', disabled: true }]);
    expect(h).not.toContain('aria-expanded');
    expect(h.match(/<button[^>]*\bdisabled/g)).toHaveLength(1);
  });
});
