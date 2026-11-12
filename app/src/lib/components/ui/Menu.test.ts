import { describe, expect, it } from 'vitest';
import src from './Menu.svelte?raw';
import overlays from '../overlays/Overlays.svelte?raw';

describe('menu groups', () => {
  it('toggles a group row in place without closing the menu', () => {
    expect(src).toMatch(/\{:else if it\.more && !child\}\s*<button[^>]*aria-expanded=\{expanded\.has\(index\)\} onclick=\{\(\) => toggle\(index\)\}/);
  });

  it('keeps the menu on screen once a group unfolds, without moving focus', () => {
    const fx = src.slice(src.indexOf('if (!menu || !expanded.size)'), src.indexOf('function toggle'));
    expect(fx).toMatch(/innerHeight - r\.height - 8/);
    expect(fx).not.toMatch(/focus\(/);
  });

  it('starts every newly opened menu folded', () => {
    expect(overlays).toMatch(/\{#key app\.menu\}<Menu \{\.\.\.app\.menu\}/);
  });
});
