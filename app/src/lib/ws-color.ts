import type { GroupId } from './grouping';
import type { SessionState } from './session-state';
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

const { pushed: _pushed, ...WORKSPACE_COLORS } = GROUP_COLOR;

export const WS_COLOR: Record<WsState, string> = WORKSPACE_COLORS;
