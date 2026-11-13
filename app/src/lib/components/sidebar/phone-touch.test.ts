import { describe, expect, it } from 'vitest';
import { declOf, parseStyle } from '../../../test/css';
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
const PHONE = ':global(:root.phone)';

function px(value: string | undefined): number {
  if (!value) return 0;
  return value === 'var(--touch)' ? TOUCH : parseFloat(value);
}

const phone = (src: string, selector: string, prop: string) => declOf(src, `${PHONE} ${selector}`, prop);
const tall = (src: string, sel: string) => Math.max(px(phone(src, sel, 'height')), px(phone(src, sel, 'min-height')));

const sources: Record<string, string> = { Sidebar: sidebar, SpaceBlock: spaceBlock, SpaceFolder: spaceFolder, SessionRow: sessionRow, WorkspaceFrame: frame, SpacesView: spacesView, AttentionBanner: banner, StatesView: statesView };

describe('CSS guard: workspace drawer touch targets on a phone', () => {
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

  it('gives the sticky note button a thumb-sized hit area', () => {
    expect(px(phone(sessionRow, '.note', 'width'))).toBeGreaterThanOrEqual(MIN);
    expect(px(phone(sessionRow, '.note', 'height'))).toBeGreaterThanOrEqual(MIN);
  });

  it('grows the note hit area on touch desktop layouts without growing the glyph', () => {
    expect(declOf(sessionRow, '.note::after', 'inset', '@media (hover: none)')).toBe('-10px');
    expect(declOf(sessionRow, '.note', 'width')).toBe('20px');
  });

  it('gives the expand chevron and the start button a thumb-sized width', () => {
    expect(px(phone(spaceBlock, '.chev', 'width'))).toBeGreaterThanOrEqual(36);
    expect(px(phone(spaceBlock, '.start', 'width'))).toBeGreaterThanOrEqual(MIN);
    expect(px(phone(spacesView, '.add', 'width'))).toBeGreaterThanOrEqual(MIN);
  });

  it('keeps group and bucket rows from being squeezed by the scrolling column', () => {
    expect(phone(spacesView, '.group', 'flex')).toBe('none');
    expect(phone(statesView, '.bucket', 'flex')).toBe('none');
  });

  it('opens a wider drawer on a phone', () => {
    expect(declOf(shell, '.drawer', 'width')).toBe('min(92vw, 400px)');
  });

  it('disables the native touch callout on the sidebar, so the long-press menu is ours', () => {
    expect(declOf(sidebar, '.sidebar', '-webkit-touch-callout')).toBe('none');
  });

  it('scopes every phone rule to :root.phone, never to a coarse pointer media query', () => {
    for (const src of Object.values(sources)) {
      for (const r of parseStyle(src)) {
        expect(r.at).not.toContain('pointer: coarse');
        expect(r.selector).not.toMatch(/^:global\(\.phone\)/);
      }
    }
  });
});
