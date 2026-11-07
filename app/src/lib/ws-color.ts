import type { WsState } from './workspaces';

// One colour per aggregated workspace state, shared with the session row palette.
export const WS_COLOR: Record<WsState, string> = {
  needs_input: 'var(--state-attention)',
  running: 'var(--state-running)',
  needs_testing: 'var(--st-testing)',
  ready: 'var(--state-open)',
  paused: 'var(--state-paused)',
  stopped: 'var(--state-closed)',
};
