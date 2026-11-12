import { describe, expect, it } from 'vitest';
import { GROUP_COLOR, PALETTE_GROUP_COLOR, PALETTE_HELD_COLOR, STATE_COLOR, WS_COLOR } from './ws-color';

describe('workspace colours', () => {
  it('colours every group in the sidebar and in the palette', () => {
    expect(Object.keys(PALETTE_GROUP_COLOR).sort()).toEqual(Object.keys(GROUP_COLOR).sort());
  });

  it('colours the palette from CSS tokens, sharing one colour for the held groups', () => {
    for (const g of Object.keys(GROUP_COLOR) as (keyof typeof GROUP_COLOR)[]) {
      expect(PALETTE_GROUP_COLOR[g], g).toMatch(/^var\(--[\w-]+\)$/);
    }
    for (const g of ['pushed', 'ready', 'stopped'] as const) expect(PALETTE_GROUP_COLOR[g]).toBe(PALETTE_HELD_COLOR);
  });

  it('derives the workspace colours from the groups without pushed', () => {
    expect(WS_COLOR).not.toHaveProperty('pushed');
    expect(WS_COLOR.running).toBe(GROUP_COLOR.running);
    expect(STATE_COLOR.running).toBe(GROUP_COLOR.running);
  });
});
