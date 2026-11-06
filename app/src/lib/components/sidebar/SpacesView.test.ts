import { describe, expect, it } from 'vitest';
import src from './SpacesView.svelte?raw';
import sidebarSrc from './Sidebar.svelte?raw';

describe('SpacesView wiring', () => {
  it('opens a session or a banner entry with app.select, and an empty chip with selectWorkspace', () => {
    // Given the component source
    // When the handlers are read
    // Then each click reaches today's action
    expect(src).toMatch(/onselect=\{\(e\) => app\.select\(e\.key\)\}/);
    expect(src).toMatch(/onselect=\{\(key\) => app\.select\(key\)\}/);
    expect(src).toMatch(/onclick=\{\(\) => app\.selectWorkspace\(w\.id\)\}/);
  });

  it('persists the expanded set through toggleOpen and runs one visibility-gated clock', () => {
    // Given the component source
    // When it is read
    // Then expand state goes through the pure helper and the clock is the shared one
    expect(src).toMatch(/saveSettings\(\{ spacesOpen: toggleOpen\(/);
    expect(src).toContain('visibleClock(');
    expect(src).not.toContain('setInterval');
  });

  it('tells an empty filter result from an empty list, and derives the host from the connection', () => {
    // Given the component source
    // When it is read
    // Then the no-match text is common.none and the host comes from hostFor
    expect(src).toMatch(/q \? t\('common\.none'\) : t\('ws\.empty'\)/);
    expect(src).toContain('hostFor(app.conn)');
  });
});

describe('Sidebar compact toggle', () => {
  it('defaults to compact for any value but tree, and hides "show archived" there', () => {
    // Given the sidebar source
    // When the layout logic is read
    // Then only an explicit tree switches layout and the archive button is tree-only
    expect(sidebarSrc).toContain("settings.workspacesLayout !== 'tree'");
    expect(sidebarSrc).toMatch(/\{#if !compact\}\s*<button[^>]*\s*class="icon-btn"\s*class:active=\{showArchived\}/);
    expect(sidebarSrc).toMatch(/saveSettings\(\{ workspacesLayout: compact \? 'tree' : 'spaces' \}\)/);
  });
});
