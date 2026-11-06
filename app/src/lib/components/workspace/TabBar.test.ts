import { describe, expect, it } from 'vitest';
import bar from './TabBar.svelte?raw';

function rule(selector: string): string {
  const style = bar.slice(bar.indexOf('<style'));
  const at = style.search(new RegExp(`\\n\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*[,{]`));
  expect(at).toBeGreaterThan(-1);
  return style.slice(at, style.indexOf('}', at));
}

describe('tab strip', () => {
  it('keeps tabs at their own width so the strip scrolls instead of squeezing them', () => {
    expect(rule('.tab')).toMatch(/^\s*flex:\s*none;/m);
    expect(rule('.tabs')).toMatch(/overflow-x:\s*auto/);
  });

  it('caps an inactive tab and lets the active one show its full title', () => {
    expect(rule('.tab')).toMatch(/^\s*max-width:\s*\d+px;/m);
    expect(rule('.tab.active')).toMatch(/max-width:\s*none/);
  });

  it('hides the scrollbar, which WebKitGTK draws over the titles', () => {
    expect(rule('.tabs')).toMatch(/scrollbar-width:\s*none/);
    expect(rule('.tabs::-webkit-scrollbar')).toMatch(/display:\s*none/);
    expect(bar).not.toMatch(/\.tabs:hover/);
  });

  it('lays no overlay over the strip on hover', () => {
    expect(bar).not.toMatch(/TabFullTitle|onpointerenter/);
  });

  it('puts the number and the close button in one slot, so hovering does not change the width', () => {
    expect(bar).toMatch(/<span class="slot"><span class="num[^"]*">.*?<\/span><button class="x"/);
    expect(rule('.slot > *')).toMatch(/grid-area:\s*1 \/ 1/);
    expect(rule('.tab:hover .num')).toMatch(/visibility:\s*hidden/);
    expect(rule('.x')).not.toMatch(/z-index/);
  });

  it('moves the pane toolbar under the tabs in a narrow pane instead of squeezing the strip', () => {
    expect(rule('.tabbar-box')).toMatch(/container-type:\s*inline-size/);
    expect(bar).toMatch(/@container \(max-width:\s*\d+px\)/);
    expect(bar.slice(bar.indexOf('@container'))).toMatch(/\.tabs\s*\{[^}]*flex:\s*1 0 100%/);
    expect(rule('.actions')).toMatch(/flex:\s*none/);
  });

  it('hands keyboard focus to the session after a mouse click on its tab, never on touch or Enter', () => {
    expect(bar).toMatch(/e\.detail > 0 && !coarsePointer\(\) && tabInfo\(key, app\.items\)\?\.item/);
    expect(bar).toMatch(/requestAnimationFrame\(\(\) => focusSession\(pane\)\)/);
    expect(bar).toMatch(/onkeydown=\{\(e\) => e\.key === 'Enter' && activate\(key\)\}/);
  });
});

describe('new tab menu', () => {
  it('only starts sessions; views live in the command palette', () => {
    const menu = bar.slice(bar.indexOf('function addMenu'), bar.indexOf('function openAdd'));
    expect(menu).toContain('app.newSession');
    for (const view of ['browser', 'graph', 'worktrees', 'system', 'usage', 'prs', 'reviewKey', 'tabs.reviewChanges']) expect(menu).not.toContain(view);
  });
});
