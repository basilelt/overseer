import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { setClient } from '../daemon/client';
import { MockDaemon } from '../daemon/mock';
import { mergeFetched, touchedKey, type Touched } from '../reduce';
import { app } from '../state.svelte';
import type { DaemonEvent, HostTarget } from '../types';

type R = { id: string; v: number };

const list = (n: number): R[] => Array.from({ length: n }, (_, i) => ({ id: `r${i}`, v: 0 }));

function touchedSome(n: number, kind = 'line'): Touched {
  const t: Touched = new Map();
  for (let i = 0; i < n; i += 10) t.set(`${kind}:r${i}`, 2);
  return t;
}

function keyCalls(n: number, touched: Touched, kind = 'line') {
  let calls = 0;
  const key = (r: R) => (calls++, r.id);
  mergeFetched(list(n), list(n).map((r) => ({ ...r, v: 1 })), kind, key, touched, 1);
  return calls;
}

describe('mergeFetched at scale', () => {
  it('takes the fetched list untouched and without a key lookup when no event touched the kind', () => {
    const fetched = list(5000);
    let calls = 0;
    const out = mergeFetched(fetched, list(5000), 'line', (r) => (calls++, r.id), touchedSome(5000, 'task'), 1);
    expect(out).toBe(fetched);
    expect(calls).toBe(0);
  });

  it('does key work linear in the list size with a tenth of the records touched', () => {
    const small = keyCalls(500, touchedSome(500));
    const big = keyCalls(5000, touchedSome(5000));
    expect(big).toBeLessThanOrEqual(small * 10 + 10);
  });

  it('returns every fetched record once, the touched ones event-applied, at 10x size', () => {
    const n = 5000;
    const out = mergeFetched(list(n), list(n).map((r) => ({ ...r, v: 1 })), 'line', (r) => r.id, touchedSome(n), 1);
    expect(out.length).toBe(n);
    expect(out.filter((r) => r.v === 1).length).toBe(n / 10);
  });

  it('touchedKey does constant work whatever the payload size', () => {
    const big = { line: { id: 'a', blob: 'x'.repeat(1_000_000) } };
    const e: DaemonEvent = { id: '1', type: 'line.updated', at: '', data: big };
    expect(touchedKey(e)).toBe('line:a');
  });
});

describe('onEvent while a refresh is in flight', () => {
  const local: HostTarget = { kind: 'local' } as HostTarget;

  const m = new MockDaemon();
  const request = m.request.bind(m);

  beforeAll(() => setClient(m));

  afterEach(async () => {
    m.request = request;
    await app.disconnect();
  });

  async function inFlight() {
    await app.connect(local);
    const lines = await m.request<{ line: { id: string } }[]>('GET', '/v1/lines');
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    m.request = (async (method: string, path: string, body?: unknown) =>
      method === 'GET' && path.split('?')[0] === '/v1/lines' ? gate.then(() => lines) : request(method, path, body)) as typeof m.request;
    return { lines, release, pending: app.refresh() };
  }

  const touched = () => (app as unknown as { touched: Touched }).touched;

  it('remembers one entry per record touched, however many events repeat it', async () => {
    const { lines, release, pending } = await inFlight();
    const ids = lines.slice(0, 3).map((l) => l.line.id);
    for (let round = 0; round < 50; round++) for (const id of ids) m.emit('line.updated', { line: { id, branch: `b${round}` } });
    expect(touched().size).toBe(ids.length);
    release();
    await pending;
  });

  it('remembers nothing once the refresh ended or while none runs', async () => {
    const { lines, release, pending } = await inFlight();
    m.emit('line.updated', { line: { id: lines[0].line.id, branch: 'x' } });
    release();
    await pending;
    expect(touched().size).toBe(0);
    m.emit('line.updated', { line: { id: lines[0].line.id, branch: 'y' } });
    expect(touched().size).toBe(0);
  });
});
