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
});
