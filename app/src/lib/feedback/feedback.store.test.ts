import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { feedback, ft, wasReported } from './feedback.svelte';
import { loadLocale, setLocale } from '../i18n.svelte';
import type { DaemonClient } from '../daemon/client';
import type { Term } from '../types';

function fakeClient() {
  let onData: ((b: Uint8Array) => void) | null = null;
  let onClose: (() => void) | undefined;
  const close = vi.fn();
  const client = {
    attachTerm: vi.fn(async (_id: string, d: (b: Uint8Array) => void, c?: () => void) => {
      onData = d;
      onClose = c;
      return { write: () => {}, resize: () => {}, close };
    }),
  } as unknown as DaemonClient;
  return { client, close, emit: (s: string) => onData?.(new TextEncoder().encode(s)), end: () => onClose?.() };
}

function memoryStorage() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k), clear: () => m.clear() };
}

const term = (id: string, over: Partial<Term> = {}): Term => ({ id, pid: 1, cwd: '/p', cmd: ['claude', '--model', 'x'], title: 'Feedback', cols: 120, rows: 32, createdAt: '', exited: false, ...over });
const URL5 = 'ISSUE_URL=https://github.com/o/r/issues/5\r\n';

beforeEach(() => {
  vi.stubGlobal('localStorage', memoryStorage());
});

afterEach(() => {
  feedback.stopAll();
  vi.unstubAllGlobals();
});

describe('feedback resume after reload', () => {
  it('re-attaches to running feedback terms only', () => {
    const f = fakeClient();
    feedback.resume(f.client, [term('r1'), term('r2', { exited: true }), term('r3', { title: 'zsh' }), term('r4', { taskId: 't' })], vi.fn());
    expect(f.client.attachTerm).toHaveBeenCalledTimes(1);
    expect(f.client.attachTerm).toHaveBeenCalledWith('r1', expect.any(Function), expect.any(Function));
    expect(feedback.watching).toEqual(['r1']);
  });

  it('announces the link once with the toast and result action', async () => {
    const f = fakeClient();
    const notify = vi.fn();
    feedback.resume(f.client, [term('r5')], notify);
    await vi.waitFor(() => expect(f.client.attachTerm).toHaveBeenCalled());
    f.emit(URL5);
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify.mock.calls[0][0]).toBe(ft('created'));
    expect(feedback.result).toEqual({ url: 'https://github.com/o/r/issues/5', comment: false });
    notify.mock.calls[0][2].run();
    expect(feedback.open).toBe(true);
    expect(feedback.watching).toEqual([]);
    feedback.close();
  });

  it('does not announce again when the scrollback is replayed after another reload', async () => {
    const first = fakeClient();
    await feedback.watch(first.client, 'r6', vi.fn());
    first.emit(URL5);
    expect(wasReported('r6')).toBe(true);
    const again = fakeClient();
    const notify = vi.fn();
    feedback.resume(again.client, [term('r6')], notify);
    expect(again.client.attachTerm).not.toHaveBeenCalled();
    await feedback.watch(again.client, 'r6', notify);
    again.emit(URL5);
    expect(notify).not.toHaveBeenCalled();
  });

  it('keeps watching when the replayed scrollback has no link yet', async () => {
    const f = fakeClient();
    const notify = vi.fn();
    feedback.resume(f.client, [term('r7')], notify);
    await vi.waitFor(() => expect(f.client.attachTerm).toHaveBeenCalled());
    f.emit(Array.from({ length: 150 }, (_, i) => `ISSUE_URL=noise-${i}\r\n`).join(''));
    expect(notify).not.toHaveBeenCalled();
    expect(feedback.watching).toEqual(['r7']);
    f.emit(URL5);
    expect(notify).toHaveBeenCalledTimes(1);
  });

  it('does not attach twice on a second resume', () => {
    const f = fakeClient();
    feedback.resume(f.client, [term('r8')], vi.fn());
    feedback.resume(f.client, [term('r8')], vi.fn());
    expect(f.client.attachTerm).toHaveBeenCalledTimes(1);
  });

  it('detaches when the terminal exits', async () => {
    const f = fakeClient();
    const notify = vi.fn();
    feedback.resume(f.client, [term('r9')], notify);
    await vi.waitFor(() => expect(f.client.attachTerm).toHaveBeenCalled());
    f.end();
    expect(feedback.watching).toEqual([]);
    expect(notify).not.toHaveBeenCalled();
  });

  it('survives a broken localStorage', async () => {
    vi.stubGlobal('localStorage', { getItem: () => { throw new Error('denied'); }, setItem: () => { throw new Error('denied'); } });
    const f = fakeClient();
    const notify = vi.fn();
    feedback.resume(f.client, [term('r10')], notify);
    await vi.waitFor(() => expect(f.client.attachTerm).toHaveBeenCalled());
    f.emit(URL5);
    expect(notify).toHaveBeenCalledTimes(1);
  });

  it('keeps only the latest reported ids', async () => {
    for (let i = 0; i < 105; i++) {
      const f = fakeClient();
      await feedback.watch(f.client, `m${i}`, vi.fn());
      f.emit(URL5);
    }
    expect(wasReported('m0')).toBe(false);
    expect(wasReported('m104')).toBe(true);
  });
});

describe('feedback store', () => {
  it('reports the issue URL once and detaches', async () => {
    const f = fakeClient();
    const found = vi.fn();
    await feedback.watch(f.client, 't1', found);
    expect(feedback.watching).toEqual(['t1']);
    f.emit('working…\r\n');
    expect(found).not.toHaveBeenCalled();
    f.emit('ISSUE_URL=https://github.com/o/r/issues/5\r\n');
    f.emit('ISSUE_URL=https://github.com/o/r/issues/6\r\n');
    expect(found).toHaveBeenCalledTimes(1);
    expect(found).toHaveBeenCalledWith({ url: 'https://github.com/o/r/issues/5', comment: false });
    expect(f.close).toHaveBeenCalled();
    expect(feedback.watching).toEqual([]);
  });

  it('stops watching when the term ends', async () => {
    const f = fakeClient();
    const found = vi.fn();
    await feedback.watch(f.client, 't2', found);
    f.end();
    expect(feedback.watching).toEqual([]);
    expect(found).not.toHaveBeenCalled();
  });

  it('does not attach twice to the same term', async () => {
    const f = fakeClient();
    await feedback.watch(f.client, 't3', () => {});
    await feedback.watch(f.client, 't3', () => {});
    expect(f.client.attachTerm).toHaveBeenCalledTimes(1);
    feedback.stopAll();
    expect(feedback.watching).toEqual([]);
  });

  it('opens with a prefill and shows results', () => {
    feedback.show({ kind: 'idea' });
    expect(feedback.open).toBe(true);
    expect(feedback.prefill?.kind).toBe('idea');
    feedback.showResult({ url: 'u', comment: true });
    expect(feedback.result?.comment).toBe(true);
    feedback.close();
    expect(feedback.open).toBe(false);
  });

  it('is translated', async () => {
    setLocale('fr');
    await loadLocale('fr');
    expect(ft('title')).toBe('Envoyer un retour');
    setLocale('en');
    expect(ft('send', { agent: 'Claude Code' })).toBe('Send with Claude Code');
  });
});
