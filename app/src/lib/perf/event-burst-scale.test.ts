import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    onmessage: ((m: unknown) => void) | null = null;
  },
}));

import { EVENT_BATCH_MS } from '../daemon/event-batch';
import { TauriDaemon } from '../daemon/tauri-client';
import { app } from '../state.svelte';
import type { DaemonEvent, LocalLine } from '../types';

const at = '2026-10-29T10:00:00Z';
const lines = (n: number): LocalLine[] =>
  Array.from({ length: n }, (_, i) => ({ line: { id: `l${i}`, agent: 'claude', state: 'active', lastActivityAt: at, worktreePath: `/w/${i}` }, local: { title: `s${i}` } })) as LocalLine[];

const updates = (k: number): DaemonEvent[] =>
  Array.from({ length: k }, (_, i) => ({
    id: String(i),
    type: 'line.updated',
    at,
    data: { line: { id: `l${i}`, agent: 'claude', state: 'active', lastActivityAt: at, worktreePath: `/w/${i}` }, local: { title: `renamed ${i}` } },
  })) as DaemonEvent[];

const snapshot = app.data;
let stop: (() => void) | undefined;

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockResolvedValue(1);
});

afterEach(() => {
  stop?.();
  stop = undefined;
  app.data = snapshot;
  invoke.mockReset();
  vi.useRealTimers();
});

// The desktop shell hands each daemon event to the page as its own task, and the page redraws
// after every task: each snapshot of the data it finds published then costs one derivation of the
// session list and of everything built on it, whatever the number of sessions.
function published(n: number, k: number): number {
  stop?.();
  app.data = { ...snapshot, lines: lines(n) };
  stop = new TauriDaemon().subscribe(
    (e) => app.onEvent(e),
    (events) => app.onEvents(events),
  );
  const channel = invoke.mock.calls.filter((c) => c[0] === 'daemon_events').at(-1)![1].onEvent as { onmessage: (m: unknown) => void };
  const redrawn = new Set<unknown>([app.data]);
  for (const e of updates(k)) {
    channel.onmessage(e);
    redrawn.add(app.data);
  }
  vi.advanceTimersByTime(EVENT_BATCH_MS);
  redrawn.add(app.data);
  return redrawn.size - 1;
}

describe('a burst of daemon events is published per burst, not per event', () => {
  it('costs the same for 10 and for 1000 events', () => {
    const small = published(500, 10);
    const big = published(500, 1_000);
    expect(small).toBe(2);
    expect(big, `published ${small} -> ${big}`).toBe(small);
  });

  it('stays flat with ten times the sessions', () => {
    expect(published(5_000, 1_000)).toBe(2);
  });

  it('applies every event of the burst', () => {
    published(500, 300);
    expect(app.data.lines.filter((l) => l.local?.title?.startsWith('renamed'))).toHaveLength(300);
    expect(app.data.lines[299]!.local?.title).toBe('renamed 299');
  });
});
