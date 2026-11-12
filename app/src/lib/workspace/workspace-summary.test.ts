import { describe, expect, it } from 'vitest';
import type { SessionState } from '../session/session-state';
import { workspaceSummary } from './workspace-summary';

const it_ = (state: SessionState, kind = 'claude') => ({ state, kind });

describe('workspaceSummary', () => {
  it('counts every state and the total', () => {
    const s = workspaceSummary([it_('attention'), it_('running'), it_('running'), it_('open'), it_('paused'), it_('closed'), it_('closed')]);
    expect(s.total).toBe(7);
    expect(s.counts).toEqual({ attention: 1, running: 2, open: 1, paused: 1, closed: 2 });
  });

  it('is empty, with no urgent state, for a workspace without sessions', () => {
    expect(workspaceSummary([])).toEqual({ total: 0, counts: { attention: 0, running: 0, open: 0, paused: 0, closed: 0 }, urgent: null });
  });

  it('ranks attention above running above open above paused above closed', () => {
    const order: SessionState[] = ['attention', 'running', 'open', 'paused', 'closed'];
    for (let i = 0; i < order.length; i++) {
      // Every state from i down is present, so the most urgent one is order[i].
      expect(workspaceSummary(order.slice(i).map((s) => it_(s))).urgent).toBe(order[i]);
    }
  });

  it('does not let many calm sessions outrank one that needs attention', () => {
    expect(workspaceSummary([...Array.from({ length: 5 }, () => it_('running')), it_('attention')]).urgent).toBe('attention');
  });

  it('ignores plain shells, which are not agent sessions', () => {
    const s = workspaceSummary([it_('running', 'shell'), it_('paused')]);
    expect([s.total, s.urgent]).toEqual([1, 'paused']);
  });
});
