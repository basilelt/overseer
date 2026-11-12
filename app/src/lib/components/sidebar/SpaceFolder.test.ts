import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { SpaceNode } from '../../workspace/spaces';
import type { Folder } from '../../types';
import type { WorkspaceSummary } from '../../workspace/workspace-summary';
import SpaceFolder from './SpaceFolder.svelte';

type FolderNode = Extract<SpaceNode, { type: 'folder' }>;
const summary = (attention = 0, running = 0, paused = 0, closed = 0): WorkspaceSummary => ({
  total: attention + running + paused + closed,
  counts: { attention, running, open: 0, paused, closed },
  urgent: null,
});
const folder = { id: 'e', name: 'Endurance', parentId: null, rank: 1, collapsed: false } as Folder;
const node = (over: Partial<FolderNode> = {}): FolderNode => ({ type: 'folder', key: 'f:e', folder, depth: 0, open: true, folders: 2, spaces: 4, summary: summary(1, 2, 1, 3), ...over });
const html = (n: FolderNode, extra: Record<string, unknown> = {}) => render(SpaceFolder, { props: { node: n, ontoggle: () => {}, ...extra } }).body;

describe('SpaceFolder', () => {
  it('shows a top-level folder as an expanded section title with its folder and workspace counts', () => {
    const h = html(node());
    expect(h).toContain('class="section');
    expect(h).toContain('aria-expanded="true"');
    expect(h).toContain('>Endurance<');
    expect(h).toContain('2 folders · 4 workspaces');
    expect(h).not.toContain('data-pill');
  });

  it('adds the pills and total of the whole subtree to a collapsed section', () => {
    const h = html(node({ open: false }));
    expect(h).toContain('aria-expanded="false"');
    for (const s of ['attention', 'running', 'paused']) expect(h).toContain(`data-pill="${s}"`);
    expect(h).toMatch(/class="total[^"]*">7</);
  });

  it('uses the singular forms and leaves out a zero folder count', () => {
    expect(html(node({ folders: 1, spaces: 1 }))).toContain('1 folder · 1 workspace');
    const h = html(node({ folders: 0, spaces: 2 }));
    expect(h).toContain('2 workspaces');
    expect(h).not.toContain('folders');
  });

  it('labels the pinned section and gives it no toggle', () => {
    const h = html(node({ key: 'f:*', folder: null }));
    expect(h).toContain('Pinned');
    expect(h).not.toContain('aria-expanded');
  });

  it('shows a nested folder as a row with an open or closed folder icon, its pills and total', () => {
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
    expect(html(node({ depth: 1, summary: summary() }))).not.toContain('class="total');
  });
});

describe('SpaceFolder interactions', () => {
  it('is a drop zone for its folder and shows the drop marker', () => {
    const h = html(node(), { dropPos: 'inside' });
    expect(h).toContain('data-drop="folder"');
    expect(h).toContain('data-id="e"');
    expect(h).toMatch(/class="[^"]*inside/);
    const row = html(node({ depth: 1 }), { dropPos: 'before' });
    expect(row).toContain('data-drop="folder"');
    expect(row).toMatch(/class="[^"]*drop-before/);
  });

  it('makes the pinned section a drop zone for pinning, with the highlight while hovered', () => {
    expect(html(node({ folder: null }), { target: true })).toMatch(/class="[^"]*target/);
    expect(html(node({ folder: null }))).not.toMatch(/class="[^"]*target/);
    expect(html(node({ folder: null }))).toContain('data-drop="pinned"');
  });
});

