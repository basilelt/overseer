import { describe, expect, it } from 'vitest';
import src from './StatesView.svelte?raw';
import sidebarSrc from './Sidebar.svelte?raw';

describe('StatesView wiring', () => {
  it('opens banner entries with app.select and rows through the helpers shared with the Espaces view', () => {
    // Given the source
    // When the handlers are read
    // Then the banner reaches app.select, and rows use selectSession (modifier clicks) and itemMenuAt (right-click)
    expect(src).toMatch(/onselect=\{\(e\) => app\.select\(e\.key\)\}/);
    expect(src).toContain('onselect={selectSession}');
    expect(src).toContain('onmenu={itemMenuAt}');
  });

  it('builds sections from byState with the visible workspaces and one shared clock', () => {
    // Given the source
    // When read
    // Then it derives from byState, uses visibleIds and visibleClock and owns no timer
    expect(src).toMatch(/byState\(.*visibleIds\(/s);
    expect(src).toContain('visibleClock(');
    expect(src).not.toContain('setInterval');
  });

  it('collapses the closed intervals by default and stores the opened ones per host', () => {
    // Given the source
    // When read
    // Then intervals are toggles persisted under statesOpen[hostKey]
    expect(src).toMatch(/aria-expanded=\{/);
    expect(src).toMatch(/saveSettings\(\{ statesOpen: \{ \.\.\.settings\.statesOpen, \[hostKey\(app\.conn\.target\)\]: toggleOpen\(/);
    expect(src).toContain('hostFor(app.conn)');
  });

  it('skips empty sections and tells a filter without match from an empty list', () => {
    // Given the source
    // When read
    // Then empty sections are not rendered
    expect(src).toMatch(/\{#if s\.count\}/);
    expect(src).toMatch(/q \? t\('common\.none'\) : t\('sidebar\.empty'\)/);
  });

  it('is what the by-state tab shows', () => {
    // Given the sidebar source
    // When read
    // Then StatesView replaces StatusList
    expect(sidebarSrc).toContain('<StatesView');
    expect(sidebarSrc).not.toContain('<StatusList');
  });
});
