import { describe, expect, it } from 'vitest';
import bar from './TabBar.svelte?raw';
import actions from '../../wsactions.ts?raw';

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

  it('shows the session cost on its tab, shrinking before the title does', () => {
    expect(bar).toMatch(/\{#if costOf\(tab\)\}.*<span class="chip cost mono" title=\{t\('cost\.tab'/);
    expect(rule('.chip.cost')).toMatch(/flex:\s*0 8 auto/);
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

  it('keeps the toolbar off the tab strip with a separator instead of letting a clipped tab touch the toggle', () => {
    expect(rule('.actions')).toMatch(/margin-left:\s*6px/);
    expect(rule('.actions')).toMatch(/border-left:\s*1px solid var\(--border\)/);
    expect(bar.slice(bar.indexOf('@container'))).toMatch(/\.actions\s*\{[^}]*border-left:\s*0/);
  });

  it('wraps the toolbar buttons in a very narrow pane so none is clipped', () => {
    expect(bar.slice(bar.indexOf('@container'))).toMatch(/\.actions\s*\{[^}]*flex-wrap:\s*wrap[^}]*min-height:\s*34px/);
  });

  it('hands keyboard focus to the session after a mouse click on its tab, never on touch or Enter', () => {
    expect(bar).toMatch(/e\.detail > 0 && !coarsePointer\(\) && tabInfo\(key, app\.items\)\?\.item/);
    expect(bar).toMatch(/requestAnimationFrame\(\(\) => focusSession\(pane\)\)/);
    expect(bar).toMatch(/onkeydown=\{\(e\) => e\.key === 'Enter' && activate\(key\)\}/);
  });
});

describe('new tab menu', () => {
  it('only starts sessions; views live in the command palette', () => {
    expect(bar).toContain("t('tabs.new'), newSessionItems())");
    const menu = actions.slice(actions.indexOf('export function newSessionItems'), actions.indexOf('export function startIn'));
    expect(menu).toContain('app.newSession');
    for (const view of ['browser', 'graph', 'worktrees', 'system', 'usage', 'prs', 'reviewKey', 'tabs.reviewChanges']) expect(menu).not.toContain(view);
  });

  it('installs an absent agent from its entry instead of greying it out', () => {
    const menu = actions.slice(actions.indexOf('export function newSessionItems'), actions.indexOf('export function startIn'));
    expect(menu).toMatch(/disabled: !a\.installed && !canInstall\(a\)/);
    expect(menu).toMatch(/run: a\.installed \? start\(a\.id\) : install\(a\.id\)/);
    expect(menu).toContain('app.installAgent');
    expect(menu).toContain("label: canInstall(a) ? t('palette.installAgent', { name: a.name }) : a.name");
  });
});
