import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { app } from '../../state.svelte';
import type { Group, Workspace } from '../../types';
import SpacesView from './SpacesView.svelte';
import src from './SpacesView.svelte?raw';
import sidebarSrc from './Sidebar.svelte?raw';

describe('SpacesView wiring', () => {
  it('opens a session or a banner entry with app.select, and an empty chip with selectWorkspace', () => {
    // Given the component source
    // When the handlers are read
    // Then each click reaches today's action
    expect(src).toMatch(/onselect=\{\(e\) => app\.select\(e\.key\)\}/);
    expect(src).toMatch(/onselect=\{select\}/);
    expect(src).toMatch(/const select = \(key: string, e\?: MouseEvent\) => !drag\.suppressed && selectSession\(key, e\)/);
    expect(src).toMatch(/onclick=\{\(\) => pick\(w\.id\)\}/);
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
  it('ignores folder chevrons while a filter is active', () => {
    // Given the component source
    // When the folder toggle is read
    // Then it returns before flipping anything when filtering
    expect(src).toMatch(/async function toggleFolder\(id: string\) \{\n    if \(drag\.suppressed\) return;\n    if \(q\) return;/);
  });

  it('renders the folder tree of folderSpaces, archived ones included on demand, with pointer drag and drop', () => {
    // Given the component source
    // When it is read
    // Then nodes come from folderSpaces over the daemon folders, and the shared pointer drag is wired on the window
    expect(src).toMatch(/folderSpaces\(spaces\(app\.ws\.workspaces, app\.items, app\.itemWs, filter, showArchived\), app\.ws\.folders, filter\)/);
    expect(src).toContain('<SpaceFolder');
    expect(src).toContain('new WsDrag()');
    expect(src).toMatch(/<svelte:window onpointermove=\{drag\.move\} onpointerup=\{drag\.end\} onpointercancel=\{drag\.cancel\}/);
    expect(src).not.toMatch(/ondragstart|ondrop=/);
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

describe('SpacesView workspace usages', () => {
  it('picks a workspace on click and opens it when closed, while the chevron only folds', () => {
    // Given the component source
    // When the pick handler is read
    // Then the header selects the workspace (or the no-workspace row) and opens it if it was closed
    expect(src).toMatch(/function pick\(id: string\)/);
    expect(src).toMatch(/app\.selectWorkspace\(id\)/);
    expect(src).toMatch(/openOnPick\(saved, app\.workspaceId, id\)/);
    expect(src).toMatch(/onpick=\{\(\) => pick\(r\.id\)\}/);
    expect(src).toMatch(/ontoggle=\{\(\) => toggle\(r\.id\)\}/);
  });

  it('ignores the click that ends a drag, on headers, folders and chips', () => {
    // Given the component source
    // When the click handlers are read
    // Then each is gated on drag.suppressed
    expect(src).toMatch(/function pick\(id: string\) \{\s*if \(drag\.suppressed\) return;/);
    expect(src).toMatch(/function toggle\(id: string\) \{\s*if \(drag\.suppressed\) return;/);
    expect(src).toMatch(/onpointercancel=\{drag\.cancel\}/);
    expect(src).toMatch(/async function toggleFolder\(id: string\) \{\s*if \(drag\.suppressed\) return;/);
  });

  it('opens the workspace, folder, group and other menus on right-click', () => {
    // Given the component source
    // When the menu wiring is read
    // Then every row kind has its menu, "Other" included
    expect(src).toMatch(/workspaceMenu\(r\.workspace\)/);
    expect(src).toMatch(/otherMenu\(\)/);
    expect(src).toMatch(/folderMenu\(f\)/);
    expect(src).toMatch(/groupMenu\(g\)/);
    expect(src).toMatch(/onitemmenu=\{itemMenuAt\}/);
  });

  it('makes the empty-workspace chips tab-drop and drag targets', () => {
    // Given the component source
    // When the chip markup is read
    // Then it carries data-workspace-id, the drop zone attributes and the tab-drop class
    expect(src).toMatch(/data-workspace-id=\{w\.id\}/);
    expect(src).toMatch(/data-drop="workspace"/);
    expect(src).toMatch(/class:tab-drop=\{app\.tabDrop === w\.id\}/);
  });
});

describe('SpacesView groups', () => {
  const group = (over: Partial<Group> = {}) => ({ id: 'g1', name: 'Release', rank: 1, members: ['a', 'b'], createdAt: '', updatedAt: '', ...over }) as Group;
  const before = { connected: app.connected, ws: app.ws, workspaceId: app.workspaceId };
  afterEach(() => Object.assign(app, before));
  const html = (filter = '') => render(SpacesView, { props: { filter, showArchived: false } }).body;

  it('shows the Groups title, the add button and the hint while there is none', () => {
    // Given a connected app without group
    app.connected = true;
    app.ws = { ...app.ws, groups: [] };
    // When the view renders
    const h = html();
    // Then the section offers the creation and its hint
    expect(h).toContain('>Groups<');
    expect(h).toContain('aria-label="New group"');
    expect(h).toContain('Group sessions from any workspace');
  });

  it('lists each group with its member count and marks the selected one', () => {
    // Given a group that is the selected space
    app.connected = true;
    app.ws = { ...app.ws, groups: [group()] };
    app.workspaceId = 'group:g1';
    // When the view renders
    const h = html();
    // Then its row shows the name and count, selected
    expect(h).toContain('>Release<');
    expect(h).toMatch(/class="[^"]*group[^"]*selected/);
    expect(h).not.toContain('Group sessions from any workspace');
  });

  it('hides the groups while filtering or offline', () => {
    // Given a group
    app.ws = { ...app.ws, groups: [group()] };
    // When filtering, then offline
    app.connected = true;
    const filtered = html('rel');
    app.connected = false;
    // Then no group section shows
    expect(filtered).not.toContain('>Release<');
    expect(html()).not.toContain('>Release<');
  });
});

describe('Sidebar workspaces layout', () => {
  it('always renders the Espaces view, with no layout toggle and no folder tree', () => {
    // Given the sidebar source
    // When the workspaces mode is read
    // Then SpacesView is the only layout and the tree and its setting are gone
    expect(sidebarSrc).toMatch(/<SpacesView \{filter\} \{showArchived\} \/>/);
    expect(sidebarSrc).not.toMatch(/WorkspaceTree|workspacesLayout|layoutTree|layoutSpaces|compact/);
  });

  it('keeps the same header buttons: add menu, new folder and the archive toggle', () => {
    // Given the sidebar source
    // When the workspaces header is read
    // Then the archive toggle is unconditional and flips showArchived
    expect(sidebarSrc).toMatch(/class:active=\{showArchived\}/);
    expect(sidebarSrc).not.toMatch(/\{#if !compact\}/);
    expect(sidebarSrc).toMatch(/onclick=\{\(\) => \(showArchived = !showArchived\)\}/);
    expect(sidebarSrc).toContain("t('ws.new')");
    expect(sidebarSrc).toContain("t('ws.newFolder')");
  });

  it('counts only listed sessions, leaving plain shells out, in the By status header', () => {
    // Given the sidebar source
    // When the By status count is read
    // Then it filters shells out of app.items instead of showing the raw length
    expect(sidebarSrc).toContain("app.items.filter((i) => i.kind !== 'shell').length");
    expect(sidebarSrc).toContain('{listed}</span>');
    expect(sidebarSrc).not.toContain('{app.items.length}');
  });
});

describe('SpacesView multi-select', () => {
  it('passes the multi selection down to the blocks', () => {
    // Given the component source
    // When the block is read
    // Then it receives app.multi
    expect(src).toContain('multi={app.multi}');
  });
});

describe('Sidebar touch menus', () => {
  it('turns a touch long press anywhere in the sidebar into the contextmenu its rows already handle', () => {
    // Given the sidebar source
    // When the root is read
    // Then one touchContextMenu action sits on the aside and iOS callouts are off
    expect(sidebarSrc).toMatch(/<aside class="sidebar"[^>]*use:touchContextMenu>/);
    expect(sidebarSrc).toMatch(/\.sidebar \{[^}]*-webkit-touch-callout: none/s);
  });
});
