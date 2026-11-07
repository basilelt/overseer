import type { Key } from './i18n.svelte';
import type { LocalLine } from './types';

// The one place that says what state an agent session is in and what the user may do with it.
// A hung-up line is not a state: it is hidden from the lists, and when one is still open in a tab its view is
// `closed` with `ended` set, every action off.
export type SessionState = 'running' | 'attention' | 'open' | 'paused' | 'closed';

// Most urgent first: how a workspace or a list ranks its sessions.
export const STATE_URGENCY: readonly SessionState[] = ['attention', 'running', 'open', 'paused', 'closed'];

export const SESSION_ACTIONS = ['interrupt', 'pause', 'cancelPause', 'resume', 'close'] as const;
export type SessionAction = (typeof SESSION_ACTIONS)[number];

export type WhyKey = Extract<Key, `session.why.${string}`>;
export interface ActionGate {
  enabled: boolean;
  reason?: WhyKey;
}

// What a running session is doing beyond its turn: a queued pause, or subagents still working.
export type SubLabel = { key: 'session.state.pause_pending' } | { key: 'session.subagents' | 'session.subagentsOne'; n: number };

export interface SessionView {
  state: SessionState;
  label: Extract<Key, `session.state.${string}`> | 'session.ended';
  sub?: SubLabel;
  // The line was hung up: shown only while its tab is still open.
  ended?: true;
  since?: string;
  actions: Record<SessionAction, ActionGate>;
}

const ON: ActionGate = { enabled: true };
const off = (reason: WhyKey): ActionGate => ({ enabled: false, reason });

export const isEnded = (l: LocalLine): boolean => l.line.state === 'hung-up';

function stateOf(l: LocalLine, termId: string | null | undefined): SessionState {
  const { line, local } = l;
  if (line.state === 'held') return local?.heldReason === 'user' ? 'paused' : 'closed';
  if (local?.pid == null && !local?.termId && !termId) return 'closed';
  if (local?.needsInput) return 'attention';
  // Subagents keep a parent working even between its own turns.
  return local?.busy || local?.holdPending || (local?.subagents ?? 0) > 0 ? 'running' : 'open';
}

// Why a session that is not mid-turn cannot be interrupted, in the words of the state it shows.
function idleWhy(local: LocalLine['local']): WhyKey {
  if ((local?.subagents ?? 0) > 0 && !local?.needsInput) return 'session.why.subagents';
  return local?.holdPending && !local.needsInput ? 'session.why.pausePending' : 'session.why.notBusy';
}

function subOf(l: LocalLine, state: SessionState): SubLabel | undefined {
  if (state !== 'running') return undefined;
  if (l.local?.holdPending) return { key: 'session.state.pause_pending' };
  const n = l.local?.subagents ?? 0;
  return n > 0 ? { key: n === 1 ? 'session.subagentsOne' : 'session.subagents', n } : undefined;
}

// termId: a live Overseer terminal attached to the line, which keeps it alive even without a pid.
export function sessionState(l: LocalLine, termId?: string | null): SessionView {
  if (isEnded(l)) {
    const ended = off('session.why.ended');
    return { state: 'closed', label: 'session.ended', ended: true, actions: { interrupt: ended, pause: ended, cancelPause: ended, resume: ended, close: ended } };
  }
  const state = stateOf(l, termId);
  const local = l.local;
  const label = `session.state.${state}` as const;
  if (state === 'paused' || state === 'closed') {
    const none = off('session.why.noProcess');
    return {
      state,
      label,
      since: state === 'paused' ? local?.heldAt ?? undefined : undefined,
      actions: { interrupt: none, pause: none, cancelPause: off('session.why.noPausePending'), resume: ON, close: ON },
    };
  }
  const pending = !!local?.holdPending;
  return {
    state,
    label,
    sub: subOf(l, state),
    actions: {
      interrupt: local?.busy && !local.needsInput ? ON : off(idleWhy(local)),
      pause: pending ? off('session.why.pausePending') : local?.pinned ? off('session.why.pinned') : ON,
      cancelPause: pending ? ON : off('session.why.noPausePending'),
      resume: off('session.why.alive'),
      close: ON,
    },
  };
}

const DOT = { attention: 'input', running: 'busy', open: 'idle', paused: 'paused', closed: 'off' } as const;

// The header dot class, derived from the same view as the label so the two never disagree.
export function stateDot(view: SessionView | null): 'input' | 'busy' | 'idle' | 'paused' | 'off' | 'ended' {
  if (!view) return 'off';
  return view.ended ? 'ended' : DOT[view.state];
}

// Whole days a session has been paused, or null when it is under a day or the time is unknown.
export function pausedDays(since: string | undefined, now: number): number | null {
  const at = since ? Date.parse(since) : NaN;
  const days = Math.floor((now - at) / 86_400_000);
  return days >= 1 ? days : null;
}
