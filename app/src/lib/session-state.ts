import type { Key } from './i18n.svelte';
import type { LocalLine } from './types';

// The one place that says what state an agent session is in and what the user may do with it.
export type SessionState = 'running' | 'needs_input' | 'ready' | 'pause_pending' | 'paused' | 'stopped' | 'closed';

export const SESSION_ACTIONS = ['interrupt', 'pause', 'cancelPause', 'resume', 'close'] as const;
export type SessionAction = (typeof SESSION_ACTIONS)[number];

export type WhyKey = Extract<Key, `session.why.${string}`>;
export interface ActionGate {
  enabled: boolean;
  reason?: WhyKey;
}

export interface SessionView {
  state: SessionState;
  label: Extract<Key, `session.state.${string}`>;
  since?: string;
  actions: Record<SessionAction, ActionGate>;
}

const ON: ActionGate = { enabled: true };
const off = (reason: WhyKey): ActionGate => ({ enabled: false, reason });

function stateOf(l: LocalLine, termId: string | null | undefined): SessionState {
  const { line, local } = l;
  if (line.state === 'hung-up') return 'closed';
  if (line.state === 'held') return local?.heldReason === 'user' ? 'paused' : 'stopped';
  if (local?.pid === undefined && !local?.termId && !termId) return 'stopped';
  if (local?.needsInput) return 'needs_input';
  if (local?.holdPending) return 'pause_pending';
  return local?.busy ? 'running' : 'ready';
}

// termId: a live Overseer terminal attached to the line, which keeps it alive even without a pid.
export function sessionState(l: LocalLine, termId?: string | null): SessionView {
  const state = stateOf(l, termId);
  const local = l.local;
  const label = `session.state.${state}` as const;
  if (state === 'closed') {
    const closed = off('session.why.closed');
    return { state, label, actions: { interrupt: closed, pause: closed, cancelPause: closed, resume: closed, close: closed } };
  }
  if (state === 'paused' || state === 'stopped') {
    const none = off('session.why.noProcess');
    return {
      state,
      label,
      since: state === 'paused' ? local?.heldAt : undefined,
      actions: { interrupt: none, pause: none, cancelPause: off('session.why.noPausePending'), resume: ON, close: ON },
    };
  }
  const pending = !!local?.holdPending;
  return {
    state,
    label,
    actions: {
      interrupt: local?.busy && !local.needsInput ? ON : off('session.why.notBusy'),
      pause: pending ? off('session.why.pausePending') : local?.pinned ? off('session.why.pinned') : ON,
      cancelPause: pending ? ON : off('session.why.noPausePending'),
      resume: off('session.why.alive'),
      close: ON,
    },
  };
}

// Whole days a session has been paused, or null when it is under a day or the time is unknown.
export function pausedDays(since: string | undefined, now: number): number | null {
  const at = since ? Date.parse(since) : NaN;
  const days = Math.floor((now - at) / 86_400_000);
  return days >= 1 ? days : null;
}
