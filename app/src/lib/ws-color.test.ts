import { describe, expect, it } from 'vitest';
import { GROUP_COLOR, PALETTE_GROUP_COLOR, PALETTE_HELD_COLOR, STATE_COLOR, WS_COLOR } from './ws-color';

describe('workspace colours', () => {
  it('colours every group in the sidebar and in the palette', () => {
    expect(Object.keys(PALETTE_GROUP_COLOR).sort()).toEqual(Object.keys(GROUP_COLOR).sort());
  });

  it('keeps the palette on its own tokens, which differ from the sidebar ones', () => {
    expect(PALETTE_GROUP_COLOR).toEqual({
      needs_input: 'var(--st-input)',
      needs_testing: 'var(--st-testing)',
      running: 'var(--st-working)',
      pushed: PALETTE_HELD_COLOR,
      ready: PALETTE_HELD_COLOR,
      paused: 'var(--st-paused)',
      stopped: PALETTE_HELD_COLOR,
    });
    expect(GROUP_COLOR.needs_input).toBe('var(--state-attention)');
  });

  it('derives the workspace colours from the groups without pushed', () => {
    expect(WS_COLOR).not.toHaveProperty('pushed');
    expect(WS_COLOR.running).toBe(GROUP_COLOR.running);
    expect(STATE_COLOR.running).toBe(GROUP_COLOR.running);
  });
});
