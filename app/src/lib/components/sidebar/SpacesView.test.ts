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
    expect(src).toMatch(/saveSettings\(\{ spacesOpen: \{ \.\.\.settings\.spacesOpen, \[hostKey\(app\.conn\.target\)\]: toggleOpen\(/);
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

describe('SpacesView folders', () => {
  it('renders the folder tree of folderSpaces and keeps the drag and drop in the tree view', () => {
    // Given the component source
    // When it is read
    // Then nodes come from folderSpaces over the daemon folders, with no drag handlers
    expect(src).toMatch(/folderSpaces\(spaces\(app\.ws\.workspaces, app\.items, app\.itemWs, filter\), app\.ws\.folders, filter\)/);
    expect(src).toContain('<SpaceFolder');
    expect(src).not.toMatch(/ondrag|ondrop/);
  });

  it('stores a folder\'s collapsed state on the daemon folder, optimistically', () => {
    // Given the component source
    // When the folder toggle is read
    // Then the local folder flips first and the daemon gets patchFolder with the new value
    expect(src).toMatch(/collapsed: !x\.collapsed/);
    expect(src).toMatch(/app\.api\.patchFolder\(id, \{ collapsed: !f\.collapsed \}\)/);
  });

  it('indents each node by its depth and gives the banner the workspace trail', () => {
    // Given the component source
    // When it is read
    // Then the depth drives the indent and the banner gets spaceTrail
    expect(src).toMatch(/style:--depth=\{n\.depth\}/);
    expect(src).toMatch(/spaceTrail\(app\.ws\)/);
    expect(src).toMatch(/pathOf=/);
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

  it('loads the folder tree only when the tree layout is chosen', () => {
    // Given the sidebar source
    // When the tree branch is read
    // Then WorkspaceTree comes from a dynamic import, never a static one
    expect(sidebarSrc).not.toMatch(/import WorkspaceTree from/);
    expect(sidebarSrc).toMatch(/\{#await import\('\.\/WorkspaceTree\.svelte'\) then \{ default: WorkspaceTree \}\}<WorkspaceTree \{filter\} \{showArchived\} \/>\{\/await\}/);
  });
});
