import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { app } from '../../state.svelte';
import { ui } from '../../mobile/ui.svelte';
import Sidebar from './Sidebar.svelte';
import sidebar from './Sidebar.svelte?raw';
import spacesView from './SpacesView.svelte?raw';
import spaceBlock from './SpaceBlock.svelte?raw';
import spaceFolder from './SpaceFolder.svelte?raw';
import sessionRow from './SessionRow.svelte?raw';
import frame from './WorkspaceFrame.svelte?raw';
import banner from './AttentionBanner.svelte?raw';
import statesView from './StatesView.svelte?raw';
import shell from '../mobile/MobileShell.svelte?raw';

const TOUCH = 44;
const MIN = 40;

function phoneRule(src: string, selector: string): string {
  const css = src.slice(src.indexOf('<style>'));
  const re = new RegExp(`:global\\(:root\\.phone\\) ${selector.replace(/[.]/g, '\\.')} \\{([^}]*)\\}`);
  const m = re.exec(css);
  if (!m) throw new Error(`no phone rule for ${selector}`);
  return m[1];
}

function px(rule: string, prop: string): number {
  const m = new RegExp(`(?:^|\\s)${prop}:\\s*([^;]+);`).exec(rule);
  if (!m) return 0;
  const v = m[1].trim();
  if (v === 'var(--touch)') return TOUCH;
  return parseFloat(v);
}

const tall = (src: string, sel: string) => Math.max(px(phoneRule(src, sel), 'height'), px(phoneRule(src, sel), 'min-height'));

describe('workspace drawer on a phone', () => {
  const sources: Record<string, string> = { Sidebar: sidebar, SpaceBlock: spaceBlock, SpaceFolder: spaceFolder, SessionRow: sessionRow, WorkspaceFrame: frame, SpacesView: spacesView, AttentionBanner: banner, StatesView: statesView };
  it.each([
    ['Sidebar', '.modes button'],
    ['SpaceBlock', '.line'],
    ['SpaceBlock', '.start'],
    ['SpaceBlock', '.more'],
    ['SpaceFolder', '.section'],
    ['SpaceFolder', '.row'],
    ['SessionRow', '.row'],
    ['WorkspaceFrame', '.head'],
    ['SpacesView', '.chip'],
    ['SpacesView', '.add'],
    ['SpacesView', '.group'],
    ['AttentionBanner', '.item'],
    ['StatesView', '.bucket'],
  ])('%s gives %s a thumb-sized height', (name, sel) => {
    expect(tall(sources[name], sel)).toBeGreaterThanOrEqual(MIN);
  });

  it('gives the expand chevron and the start button a thumb-sized width', () => {
    expect(px(phoneRule(spaceBlock, '.chev'), 'width')).toBeGreaterThanOrEqual(36);
    expect(px(phoneRule(spaceBlock, '.start'), 'width')).toBeGreaterThanOrEqual(MIN);
    expect(px(phoneRule(spacesView, '.add'), 'width')).toBeGreaterThanOrEqual(MIN);
  });

  it('keeps rows from being squeezed by the scrolling column', () => {
    expect(phoneRule(spacesView, '.group')).toMatch(/flex: none;/);
    expect(phoneRule(statesView, '.bucket')).toMatch(/flex: none;/);
  });

  it('drops the connection state from session rows, which the top bar already shows', () => {
    expect(phoneRule(sessionRow, '.hst')).toMatch(/display: none;/);
  });

  it('opens a wider drawer', () => {
    expect(shell).toMatch(/\.drawer \{[^}]*width: min\(92vw, 400px\);/);
  });

  it('scopes every phone rule to :root.phone so the desktop sidebar is unchanged', () => {
    for (const src of [sidebar, spacesView, spaceBlock, spaceFolder, sessionRow, frame, banner, statesView]) {
      const css = src.slice(src.indexOf('<style>'));
      expect(css).not.toMatch(/@media \(pointer: coarse\)/);
      expect(css).not.toMatch(/:global\(\.phone\)/);
    }
  });
});

describe('Sidebar workspace actions', () => {
  const before = { layout: ui.layout, mode: app.sidebarMode, git: app.sidebarGit };
  afterEach(() => {
    ui.layout = before.layout;
    app.sidebarMode = before.mode;
    app.sidebarGit = before.git;
  });

  const html = () => render(Sidebar).body;
  const header = (h: string) => h.slice(h.indexOf('<header'), h.indexOf('</header>'));
  const tools = (h: string) => {
    const i = h.indexOf('class="tools');
    return h.slice(i, h.indexOf('</div>', i));
  };

  it('keeps the new, folder and archive buttons in the header on desktop', () => {
    ui.layout = 'desktop';
    app.sidebarMode = 'workspaces';
    app.sidebarGit = false;
    const h = html();
    expect(header(h)).toContain('aria-label="New workspace"');
    expect(header(h)).toContain('aria-label="New folder"');
    expect(tools(h)).not.toContain('aria-label="New workspace"');
  });

  it('moves them next to the filter on a phone so the mode tabs get the full width', () => {
    ui.layout = 'phone';
    app.sidebarMode = 'workspaces';
    app.sidebarGit = false;
    const h = html();
    expect(header(h)).not.toContain('aria-label="New workspace"');
    expect(header(h)).not.toContain('class="count');
    expect(tools(h)).toContain('aria-label="New workspace"');
    expect(tools(h)).toContain('aria-label="New folder"');
  });

  it('shows the session count, not the workspace actions, in the by-status view on a phone', () => {
    ui.layout = 'phone';
    app.sidebarMode = 'status';
    app.sidebarGit = false;
    const h = html();
    expect(h).not.toContain('aria-label="New workspace"');
    expect(header(h)).toContain('class="count');
  });
});
