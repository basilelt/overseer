import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { t } from '../../i18n.svelte';
import { app } from '../../state.svelte';
import type { Folder, Group, LocalLine, Workspace } from '../../types';
import SpacesView from './SpacesView.svelte';

const ws = (id: string, over: Partial<Workspace> = {}) => ({ id, name: `Space ${id}`, path: `/p/${id}`, folderId: null, pinned: false, archived: false, auto: false, rank: 1, tabs: [], notifyIdle: false, createdAt: '', updatedAt: '', ...over }) as Workspace;
const folder = (id: string, over: Partial<Folder> = {}) => ({ id, name: `Folder ${id}`, parentId: null, rank: 1, collapsed: false, createdAt: '', updatedAt: '', ...over }) as Folder;
const before = { connected: app.connected, ws: app.ws, workspaceId: app.workspaceId, tabDrop: app.tabDrop, data: app.data, multi: app.multi };
afterEach(() => Object.assign(app, before));
const view = (workspaces: Workspace[], folders: Folder[] = [], filter = '') => {
  app.connected = true;
  app.ws = { workspaces, folders, groups: [] };
  return render(SpacesView, { props: { filter, showArchived: false } }).body;
};
const chip = (h: string, id: string) => h.match(new RegExp(`<button[^>]*data-workspace-id="${id}"[^>]*>`))?.[0];

describe('SpacesView empty workspaces', () => {
  it('shows a workspace with no session as a chip that is a drop and drag target', () => {
    const c = chip(view([ws('a'), ws('b')]), 'a')!;
    expect(c).toContain('data-drop="workspace"');
    expect(c).toContain('data-id="a"');
    expect(c).toContain('type="button"');
  });

  it('marks the active workspace chip as selected and only that one', () => {
    app.workspaceId = 'b';
    const h = view([ws('a'), ws('b')]);
    expect(chip(h, 'b')).toMatch(/class="[^"]*\bselected\b/);
    expect(chip(h, 'a')).not.toMatch(/class="[^"]*\bselected\b/);
  });

  it('marks the chip a tab is dragged over, so the drop target shows', () => {
    app.tabDrop = 'a';
    const h = view([ws('a'), ws('b')]);
    expect(chip(h, 'a')).toMatch(/class="[^"]*\btab-drop\b/);
    expect(chip(h, 'b')).not.toMatch(/class="[^"]*\btab-drop\b/);
  });

  it('gives a missing workspace its path as a tooltip and an archived one its class', () => {
    const h = view([ws('m', { missing: true }), ws('x', { archived: true }), ws('ok')]);
    expect(chip(h, 'm')).toContain('title="/p/m"');
    expect(chip(h, 'm')).toMatch(/class="[^"]*\bmissing\b/);
    expect(chip(h, 'ok')).not.toContain('title=');
    expect(chip(h, 'x')).toBeUndefined();
  });
});

describe('SpacesView folders and empty state', () => {
  it('tells an empty filter result from an empty list', () => {
    expect(view([])).toContain(t('ws.empty'));
    const filtered = view([ws('a')], [], 'zzz');
    expect(filtered).toContain(t('common.none'));
    expect(filtered).not.toContain(t('ws.empty'));
  });

  it('renders a folder as a section holding its workspaces, with no native draggable attribute', () => {
    const h = view([ws('a', { folderId: 'f1' }), ws('b')], [folder('f1')]);
    expect(h).toContain('>Folder f1<');
    expect(h.indexOf('>Folder f1<')).toBeLessThan(h.indexOf('data-workspace-id="a"'));
    expect(h).not.toMatch(/\bdraggable=/);
  });

  it('hides the workspaces of a collapsed folder, but not while filtering', () => {
    const f = [folder('f1', { collapsed: true })];
    const w = [ws('a', { folderId: 'f1' })];
    expect(chip(view(w, f), 'a')).toBeUndefined();
    expect(chip(view(w, f, 'Space'), 'a')).toContain('data-workspace-id="a"');
  });

  it('indents a nested folder by its depth', () => {
    const h = view([ws('a', { folderId: 'f2' })], [folder('f1'), folder('f2', { parentId: 'f1' })]);
    expect(h).toContain('--depth: 1;');
    expect(h).toContain('--depth: 2;');
  });

  it('lists a pinned workspace under the pinned section before the others', () => {
    const h = view([ws('a'), ws('p', { pinned: true })]);
    expect(h.indexOf('data-workspace-id="p"')).toBeLessThan(h.indexOf('data-workspace-id="a"'));
  });
});

describe('SpacesView sessions', () => {
  const line = (id: string, workspaceId: string): LocalLine => ({ line: { id, agent: 'claude', state: 'active', lastActivityAt: '2026-11-05T10:00:00Z' }, local: { title: `T-${id}`, workspaceId, pid: 1, busy: true } });
  const withLines = (workspaces: Workspace[], lines: LocalLine[]) => {
    app.data = { ...app.data, lines };
    return view(workspaces);
  };

  it('opens the active workspace and keeps the others folded, listing sessions only in the open one', () => {
    app.workspaceId = 'a';
    const h = withLines([ws('a'), ws('b')], [line('1', 'a'), line('2', 'b')]);
    expect(h).toContain('aria-label="Collapse Space a"');
    expect(h).toContain('aria-label="Expand Space b"');
    expect(h).toContain('T-1');
    expect(h).not.toContain('T-2');
  });

  it('keeps the parent of the active nested workspace open', () => {
    app.workspaceId = 'c';
    const h = withLines([ws('p'), ws('c', { parentId: 'p' })], [line('1', 'p'), line('2', 'c')]);
    expect(h).toContain('aria-label="Collapse Space p"');
    expect(h).toContain('T-1');
  });

  it('hands the multi selection down to the session rows', () => {
    app.workspaceId = 'a';
    app.multi = new Set(['line:1']);
    const h = withLines([ws('a')], [line('1', 'a'), line('2', 'a')]);
    expect(h.match(/class="row[^"]*\bmulti\b/g)).toHaveLength(1);
  });
});

describe('SpacesView groups', () => {
  const group = (over: Partial<Group> = {}) => ({ id: 'g1', name: 'Release', rank: 1, members: ['a', 'b'], createdAt: '', updatedAt: '', ...over }) as Group;
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
