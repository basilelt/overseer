import { describe, expect, it, vi } from 'vitest';
import { groupItems, sidebarItems } from '../session/grouping';
import * as machine from '../session/session-state';
import type { LocalLine } from '../types';
import { aggregate } from '../workspace/workspaces';

vi.mock('../session/session-state', async (orig) => {
  const actual = await orig<typeof import('../session/session-state')>();
  return { ...actual, sessionState: vi.fn(actual.sessionState) };
});

const lines = (n: number): LocalLine[] =>
  Array.from({ length: n }, (_, i) => ({
    line: { id: `l${i}`, agent: 'claude', state: i % 3 === 0 ? 'held' : 'active', lastActivityAt: '2026-10-29T10:00:00Z' },
    local: { pid: i % 3 === 0 ? undefined : i, busy: i % 2 === 0, heldReason: i % 6 === 0 ? 'user' : 'lost' },
  })) as LocalLine[];

function work(n: number) {
  vi.mocked(machine.sessionState).mockClear();
  const items = sidebarItems(lines(n), [], []);
  aggregate(items);
  groupItems(items);
  return vi.mocked(machine.sessionState).mock.calls.length;
}

describe('sidebar derivation scales linearly with the session count', () => {
  it('runs the state machine a constant number of times per line', () => {
    // Given 100 and 1000 sessions in mixed states
    // When the sidebar items, groups and workspace aggregate are derived
    // Then the state machine runs the same number of times per line and the work grows 10x
    const small = work(100);
    const big = work(1000);
    expect(big / small).toBeCloseTo(10, 5);
    expect(small / 100).toBeLessThanOrEqual(2);
  });
});
