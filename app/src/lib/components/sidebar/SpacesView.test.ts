import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { app } from '../../state.svelte';
import type { Group, Workspace } from '../../types';
import SpacesView from './SpacesView.svelte';
import Sidebar from './Sidebar.svelte';
import src from './SpacesView.svelte?raw';
import sidebarSrc from './Sidebar.svelte?raw';

describe('SpacesView wiring', () => {
  it('opens a session or a banner entry with app.select, and an empty chip with selectWorkspace', () => {
    expect(src).toMatch(/onselect=\{\(e\) => app\.select\(e\.key\)\}/);
    expect(src).toMatch(/onselect=\{select\}/);
    expect(src).toMatch(/const select = \(key: string, e\?: MouseEvent\) => !drag\.suppressed && selectSession\(key, e\)/);
    expect(src).toMatch(/onclick=\{\(\) => pick\(w\.id\)\}/);
  });

  it('persists the expanded set through toggleOpen and runs one visibility-gated clock', () => {
    expect(src).toMatch(/saveSettings\(\{ spacesOpen: \{ \.\.\.settings\.spacesOpen, \[hostKey\(app\.conn\.target\)\]: toggleOpen\(/);
    expect(src).toContain('visibleClock(');
    expect(src).not.toContain('setInterval');
  });

  it('tells an empty filter result from an empty list, and names no host on its rows', () => {
    expect(src).toMatch(/q \? t\('common\.none'\) : t\('ws\.empty'\)/);
    expect(src).not.toContain('hostFor');
  });
});

describe('SpacesView folders', () => {
  it('ignores folder chevrons while a filter is active', () => {
    expect(src).toMatch(/async function toggleFolder\(id: string\) \{\n    if \(drag\.suppressed\) return;\n    if \(q\) return;/);
  });

  it('renders the folder tree of folderSpaces, archived ones included on demand, with pointer drag and drop', () => {
    expect(src).toMatch(/folderSpaces\(spaces\(app\.ws\.workspaces, app\.items, app\.itemWs, filter, showArchived\), app\.ws\.folders, filter\)/);
    expect(src).toContain('<SpaceFolder');
    expect(src).toContain('new WsDrag()');
    expect(src).toMatch(/<svelte:window onpointermove=\{drag\.move\} onpointerup=\{drag\.end\} onpointercancel=\{drag\.cancel\}/);
    expect(src).not.toMatch(/ondragstart|ondrop=/);
  });

  it('stores a folder\'s collapsed state on the daemon folder, optimistically', () => {
    expect(src).toMatch(/collapsed: !x\.collapsed/);
    expect(src).toMatch(/app\.api\.patchFolder\(id, \{ collapsed: !f\.collapsed \}\)/);
  });

  it('indents each node by its depth and gives the banner the workspace trail', () => {
    expect(src).toMatch(/style:--depth=\{n\.depth\}/);
    expect(src).toMatch(/spaceTrail\(app\.ws\)/);
    expect(src).toMatch(/pathOf=/);
  });
});

describe('SpacesView workspace usages', () => {
  it('picks a workspace on click and opens it when closed, while the chevron only folds', () => {
    expect(src).toMatch(/function pick\(id: string\)/);
    expect(src).toMatch(/app\.selectWorkspace\(id\)/);
    expect(src).toMatch(/openOnPick\(saved, app\.workspaceId, id\)/);
    expect(src).toMatch(/onpick=\{\(\) => pick\(r\.id\)\}/);
    expect(src).toMatch(/ontoggle=\{\(\) => toggle\(r\.id\)\}/);
  });

  it('ignores the click that ends a drag, on headers, folders and chips', () => {
    expect(src).toMatch(/function pick\(id: string\) \{\s*if \(drag\.suppressed\) return;/);
    expect(src).toMatch(/function toggle\(id: string\) \{\s*if \(drag\.suppressed\) return;/);
    expect(src).toMatch(/onpointercancel=\{drag\.cancel\}/);
    expect(src).toMatch(/async function toggleFolder\(id: string\) \{\s*if \(drag\.suppressed\) return;/);
  });

  it('opens the workspace, folder, group and other menus on right-click', () => {
    expect(src).toMatch(/workspaceMenu\(r\.workspace\)/);
    expect(src).toMatch(/otherMenu\(\)/);
    expect(src).toMatch(/folderMenu\(f\)/);
    expect(src).toMatch(/groupMenu\(g\)/);
    expect(src).toMatch(/onitemmenu=\{itemMenuAt\}/);
  });

  it('makes the empty-workspace chips tab-drop and drag targets', () => {
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
    app.connected = true;
    app.ws = { ...app.ws, groups: [] };
    const h = html();
    expect(h).toContain('>Groups<');
    expect(h).toContain('aria-label="New group"');
    expect(h).toContain('Group sessions from any workspace');
  });

  it('lists each group with its member count and marks the selected one', () => {
    app.connected = true;
    app.ws = { ...app.ws, groups: [group()] };
    app.workspaceId = 'group:g1';
    const h = html();
    expect(h).toContain('>Release<');
    expect(h).toMatch(/class="[^"]*group[^"]*selected/);
    expect(h).not.toContain('Group sessions from any workspace');
  });

  it('hides the groups while filtering or offline', () => {
    app.ws = { ...app.ws, groups: [group()] };
    app.connected = true;
    const filtered = html('rel');
    app.connected = false;
    expect(filtered).not.toContain('>Release<');
    expect(html()).not.toContain('>Release<');
  });
});

describe('Sidebar workspaces layout', () => {
  it('always renders the Espaces view, with no layout toggle and no folder tree', () => {
    expect(sidebarSrc).toMatch(/<SpacesView \{filter\} \{showArchived\} \/>/);
    expect(sidebarSrc).not.toMatch(/WorkspaceTree|workspacesLayout|layoutTree|layoutSpaces|compact/);
  });

  it('keeps the same header buttons: add menu, new folder and the archive toggle', () => {
    expect(sidebarSrc).toMatch(/class:active=\{showArchived\}/);
    expect(sidebarSrc).not.toMatch(/\{#if !compact\}/);
    expect(sidebarSrc).toMatch(/onclick=\{\(\) => \(showArchived = !showArchived\)\}/);
    expect(sidebarSrc).toContain("t('ws.new')");
    expect(sidebarSrc).toContain("t('ws.newFolder')");
  });

  it('counts only listed sessions, leaving plain shells out, in the By status header', () => {
    const before = { data: app.data, sidebarMode: app.sidebarMode, sidebarGit: app.sidebarGit };
    const term = (id: string, agent: string) => ({ id, pid: 1, cwd: '/w', cmd: [agent], title: id, cols: 80, rows: 24, createdAt: '', exited: false, agent });
    try {
      app.data = { ...app.data, lines: [], tasks: [], terms: [term('t1', 'shell'), term('t2', 'claude'), term('t3', 'codex')] } as typeof app.data;
      app.sidebarMode = 'status';
      app.sidebarGit = false;
      expect(app.items.length).toBe(3);
      expect(render(Sidebar).body).toMatch(/<span class="count faint[^"]*">2<\/span>/);
    } finally {
      Object.assign(app, before);
    }
  });
});

describe('SpacesView multi-select', () => {
  it('passes the multi selection down to the blocks', () => {
    expect(src).toContain('multi={app.multi}');
  });
});

describe('Sidebar touch menus', () => {
  it('turns a touch long press anywhere in the sidebar into the contextmenu its rows already handle', () => {
    expect(sidebarSrc).toMatch(/<aside class="sidebar"[^>]*use:touchContextMenu>/);
    expect(sidebarSrc).toMatch(/\.sidebar \{[^}]*-webkit-touch-callout: none/s);
  });

  it('keeps the parent of the active nested workspace open, wires subjects to the same toggles and drags only real workspaces', () => {
    expect(src).toMatch(/const opened = \$derived\(new Set\(\[\.\.\.\(saved \?\? \[app\.workspaceId\]\), \.\.\.\(app\.currentWorkspace\?\.parentId \? \[app\.currentWorkspace\.parentId\] : \[\]\)\]\)\)/);
    expect(src).toMatch(/ontoggle: toggle,\s*onpick: pick,\s*onclosed: toggleClosed,/);
    expect(src).toMatch(/kids=\{kidsFor\(r\.workspace\)\}/);
    expect(src).toMatch(/drag\.start\(e, 'session', key, zone\)/);
    expect(src).toMatch(/tabs=\{tabsOf\(r\.id\)\}/);
    expect(src).toMatch(/tabsOf,/);
    expect(src).toMatch(/drag\.dragging && drag\.kind !== 'session' && !pinned/);
    expect(src).toMatch(/drag\.dragging && drag\.kind !== 'session'\}<div class="rootdrop"/);
  });
});
