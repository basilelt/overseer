import type { GroupId } from '../session/grouping';
import type { SessionState } from '../session/session-state';
import type { WsState } from './workspaces';

export const STATE_COLOR: Record<SessionState, string> = {
  running: 'var(--state-running)',
  attention: 'var(--state-attention)',
  open: 'var(--state-open)',
  paused: 'var(--state-paused)',
  closed: 'var(--state-closed)',
};

export const GROUP_COLOR: Record<GroupId, string> = {
  needs_input: 'var(--state-attention)',
  needs_testing: 'var(--st-testing)',
  running: 'var(--state-running)',
  pushed: 'var(--st-done)',
  ready: 'var(--state-open)',
  paused: 'var(--state-paused)',
  stopped: 'var(--state-closed)',
};

export const PALETTE_HELD_COLOR = 'var(--st-held)';

export const PALETTE_GROUP_COLOR: Record<GroupId, string> = {
  needs_input: 'var(--st-input)',
  needs_testing: 'var(--st-testing)',
  running: 'var(--st-working)',
  pushed: PALETTE_HELD_COLOR,
  ready: PALETTE_HELD_COLOR,
  paused: 'var(--st-paused)',
  stopped: PALETTE_HELD_COLOR,
};

const { pushed: _pushed, ...WORKSPACE_COLORS } = GROUP_COLOR;

export const WS_COLOR: Record<WsState, string> = WORKSPACE_COLORS;
