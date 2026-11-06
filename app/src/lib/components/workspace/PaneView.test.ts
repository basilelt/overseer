import { describe, expect, it } from 'vitest';
import pane from './PaneView.svelte?raw';

describe('pane tab kinds', () => {
  it('remounts the lazy tab view when the kind changes, so it never receives another kind\'s tab', () => {
    expect(pane).toMatch(/\{#key tab\.kind\}\s*\{#await TAB_KINDS\[tab\.kind\]\?\.load\?\.\(\) then View\}[\s\S]*?\{\/await\}\s*\{\/key\}/);
  });
});
