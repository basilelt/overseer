import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { SpaceNode } from '../../spaces';
import type { Folder } from '../../types';
import type { WorkspaceSummary } from '../../workspace-summary';
import SpaceFolder from './SpaceFolder.svelte';

type FolderNode = Extract<SpaceNode, { type: 'folder' }>;
const summary = (attention = 0, running = 0, paused = 0, closed = 0): WorkspaceSummary => ({
  total: attention + running + paused + closed,
  counts: { attention, running, open: 0, paused, closed },
  urgent: null,
});
const folder = { id: 'e', name: 'Endurance', parentId: null, rank: 1, collapsed: false } as Folder;
const node = (over: Partial<FolderNode> = {}): FolderNode => ({ type: 'folder', key: 'f:e', folder, depth: 0, open: true, folders: 2, spaces: 4, summary: summary(1, 2, 1, 3), ...over });
const html = (n: FolderNode) => render(SpaceFolder, { props: { node: n, ontoggle: () => {} } }).body;

describe('SpaceFolder', () => {
  it('shows a top-level folder as an expanded section title with its folder and workspace counts', () => {
    // Given an open depth-0 folder with 2 subfolders and 4 workspaces
    // When it renders
    // Then it is a section toggle carrying the name and "2 folders · 4 workspaces", without pills
    const h = html(node());
    expect(h).toContain('class="section');
    expect(h).toContain('aria-expanded="true"');
    expect(h).toContain('>Endurance<');
    expect(h).toContain('2 folders · 4 workspaces');
    expect(h).not.toContain('data-pill');
  });

  it('adds the pills and total of the whole subtree to a collapsed section', () => {
    // Given a collapsed section whose subtree holds attention, running and paused sessions
    // When it renders
    // Then the three pills and the total show next to the counts
    const h = html(node({ open: false }));
    expect(h).toContain('aria-expanded="false"');
    for (const s of ['attention', 'running', 'paused']) expect(h).toContain(`data-pill="${s}"`);
    expect(h).toMatch(/class="total[^"]*">7</);
  });

  it('uses the singular forms and leaves out a zero folder count', () => {
    // Given sections with 1 folder and 1 workspace, then no folder and 2 workspaces
    // When they render
    // Then the labels read "1 folder · 1 workspace" and "2 workspaces"
    expect(html(node({ folders: 1, spaces: 1 }))).toContain('1 folder · 1 workspace');
    const h = html(node({ folders: 0, spaces: 2 }));
    expect(h).toContain('2 workspaces');
    expect(h).not.toContain('folders');
  });

  it('labels the pinned section and gives it no toggle', () => {
    // Given the pinned section
    // When it renders
    // Then it reads Pinned and cannot be collapsed
    const h = html(node({ key: 'f:*', folder: null }));
    expect(h).toContain('Pinned');
    expect(h).not.toContain('aria-expanded');
  });

  it('shows a nested folder as a row with an open or closed folder icon, its pills and total', () => {
    // Given a depth-2 folder, open then collapsed
    // When it renders
    // Then the row carries the matching icon, the pills and the subtree total
    const open = html(node({ depth: 2 }));
    expect(open).toContain('class="row');
    expect(open).toContain('lucide-folder-open');
    expect(open).toContain('data-pill="running"');
    expect(open).toMatch(/class="total[^"]*">7</);
    const closed = html(node({ depth: 2, open: false }));
    expect(closed).not.toContain('lucide-folder-open');
    expect(closed).toContain('lucide-folder');
  });

  it('hides the total of a folder without sessions', () => {
    // Given a nested folder whose subtree has no session
    // When it renders
    // Then no total is shown
    expect(html(node({ depth: 1, summary: summary() }))).not.toContain('class="total');
  });
});
