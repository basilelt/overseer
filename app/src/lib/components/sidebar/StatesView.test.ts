import { describe, expect, it } from 'vitest';
import src from './StatesView.svelte?raw';
import sidebarSrc from './Sidebar.svelte?raw';

describe('StatesView wiring', () => {
  it('opens banner entries with app.select and rows through the helpers shared with the Espaces view', () => {
    expect(src).toMatch(/onselect=\{\(e\) => app\.select\(e\.key\)\}/);
    expect(src).toContain('onselect={selectSession}');
    expect(src).toContain('onmenu={itemMenuAt}');
  });

  it('builds sections from byState with the visible workspaces and one shared clock', () => {
    expect(src).toMatch(/byState\(.*visibleIds\(/s);
    expect(src).toContain('visibleClock(');
    expect(src).not.toContain('setInterval');
  });

  it('collapses the closed intervals by default and stores the opened ones per host', () => {
    expect(src).toMatch(/aria-expanded=\{/);
    expect(src).toMatch(/saveSettings\(\{ statesOpen: \{ \.\.\.settings\.statesOpen, \[hostKey\(app\.conn\.target\)\]: toggleOpen\(/);
    expect(src).not.toContain('hostFor');
  });

  it('skips empty sections and tells a filter without match from an empty list', () => {
    expect(src).toMatch(/\{#if s\.count\}/);
    expect(src).toMatch(/q \? t\('common\.none'\) : t\('sidebar\.empty'\)/);
  });

  it('is what the by-state tab shows', () => {
    expect(sidebarSrc).toContain('<StatesView');
  });

  it('loads the by-status view lazily and only imports it from the sidebar on demand', () => {
    expect(sidebarSrc).toContain("import('./StatesView.svelte')");
    expect(sidebarSrc).not.toMatch(/import StatesView from/);
  });
});
