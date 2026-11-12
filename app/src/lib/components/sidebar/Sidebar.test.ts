import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { app } from '../../state.svelte';
import { ui } from '../../mobile/ui.svelte';
import type { LocalLine, Term } from '../../types';
import Sidebar from './Sidebar.svelte';

const before = { layout: ui.layout, mode: app.sidebarMode, scm: app.sidebarGit, data: app.data, multi: app.multi };
afterEach(() => {
  ui.layout = before.layout;
  app.sidebarMode = before.mode;
  app.sidebarGit = before.scm;
  app.data = before.data;
  app.multi = before.multi;
});

const html = (layout: 'desktop' | 'phone', mode: 'workspaces' | 'status') => {
  ui.layout = layout;
  app.sidebarMode = mode;
  app.sidebarGit = false;
  return render(Sidebar).body;
};
const header = (h: string) => h.slice(h.indexOf('<header'), h.indexOf('</header>'));
const tools = (h: string) => {
  const i = h.indexOf('class="tools');
  return h.slice(i, h.indexOf('</div>', i));
};
const term = (id: string, cmd: string) => ({ id, exited: false, cmd: [cmd], title: cmd, cwd: '/tmp', createdAt: '2026-10-29T09:00:00Z' }) as Term;

describe('Sidebar workspace actions', () => {
  it('keeps the new, folder and archive buttons in the header on desktop', () => {
    const h = html('desktop', 'workspaces');
    for (const label of ['New workspace', 'New folder']) {
      expect(header(h)).toContain(`aria-label="${label}"`);
      expect(tools(h)).not.toContain(`aria-label="${label}"`);
    }
    expect(header(h)).toMatch(/aria-label="Show archived[^"]*"[^>]*aria-pressed="false"/);
  });

  it('moves them next to the filter on a phone so the mode tabs get the full width', () => {
    const h = html('phone', 'workspaces');
    expect(header(h)).not.toContain('aria-label="New workspace"');
    expect(header(h)).not.toContain('class="count');
    expect(tools(h)).toContain('aria-label="New workspace"');
    expect(tools(h)).toContain('aria-label="New folder"');
  });

  it('shows the session count, not the workspace actions, in the by-status view on a phone', () => {
    const h = html('phone', 'status');
    expect(h).not.toContain('aria-label="New workspace"');
    expect(header(h)).toContain('class="count');
  });

  it('counts only listed sessions in the by-status header, leaving plain shells out', () => {
    const lines: LocalLine[] = [{ line: { id: 'ln_1', agent: 'claude', state: 'active', lastActivityAt: '2026-10-29T10:00:00Z' }, local: {} }];
    app.data = { ...app.data, lines, terms: [term('s1', 'zsh'), term('s2', 'bash')] };
    expect(header(html('desktop', 'status'))).toMatch(/class="count[^"]*">1</);
    app.data = { ...app.data, lines: [], terms: [term('s1', 'zsh')] };
    expect(header(html('desktop', 'status'))).toMatch(/class="count[^"]*">0</);
  });
});

describe('Sidebar source control tab', () => {
  const tab = (h: string, selected: boolean) => h.match(new RegExp(`<button[^>]*role="tab"[^>]*class="[^"]*\\bgit\\b[^"]*"[^>]*aria-selected="${selected}"|<button[^>]*role="tab"[^>]*aria-selected="${selected}"[^>]*class="[^"]*\\bgit\\b`));

  it('sits next to the mode tabs and swaps the filter and tools for source control when selected', () => {
    const off = html('desktop', 'workspaces');
    expect(tab(off, false)).not.toBeNull();
    expect(off).toContain('class="tools');
    ui.layout = 'desktop';
    app.sidebarGit = true;
    const on = render(Sidebar).body;
    expect(tab(on, true)).not.toBeNull();
    expect(on).not.toContain('class="tools');
  });
});

describe('Sidebar selection bar', () => {
  it('appears only once more than one session is selected', () => {
    app.multi = new Set(['a']);
    expect(html('desktop', 'workspaces')).not.toContain('role="toolbar"');
    app.multi = new Set(['a', 'b']);
    expect(html('desktop', 'workspaces')).toContain('role="toolbar"');
  });
});
