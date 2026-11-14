import { describe, expect, it } from 'vitest';
import * as colors from './ws-color';
import { GROUP_COLOR, STATE_COLOR, WS_COLOR } from './ws-color';

describe('workspace colours', () => {
  it('colours every group from CSS tokens', () => {
    for (const g of Object.keys(GROUP_COLOR) as (keyof typeof GROUP_COLOR)[]) {
      expect(GROUP_COLOR[g], g).toMatch(/^var\(--[\w-]+\)$/);
    }
  });

  it('derives the workspace colours from the groups without pushed', () => {
    expect(WS_COLOR).not.toHaveProperty('pushed');
    expect(WS_COLOR.running).toBe(GROUP_COLOR.running);
    expect(STATE_COLOR.running).toBe(GROUP_COLOR.running);
  });

  it('has one colour table per concept, none specific to the palette', () => {
    expect(Object.keys(colors).sort()).toEqual(['GROUP_COLOR', 'STATE_COLOR', 'WS_COLOR']);
  });
});
