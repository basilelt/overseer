import { describe, expect, it, vi } from 'vitest';
import { runSessionAction, type ActionApi } from './session-action';

function fakeApi(plan = { action: 'held' }): ActionApi & { calls: string[] } {
  const calls: string[] = [];
  return {
    calls,
    keys: vi.fn(async (id: string, k: string[]) => void calls.push(`keys ${id} ${k.join(',')}`)),
    holdAfterTurn: vi.fn(async (id: string) => (calls.push(`holdAfterTurn ${id}`), plan)),
    cancelHold: vi.fn(async (id: string) => (calls.push(`cancelHold ${id}`), { lineId: id, holdPending: false as const })),
    close: vi.fn(async (id: string, confirm: boolean) => void calls.push(`close ${id} ${confirm}`)),
    resume: vi.fn(async (id: string) => (calls.push(`resume ${id}`), { id: 't1' })),
  };
}

describe('runSessionAction', () => {
  it('pauses through holdAfterTurn so a busy turn finishes first instead of being refused', async () => {
    const api = fakeApi({ action: 'hold-pending' });
    const r = await runSessionAction(api, 'L1', 'pause');
    expect(api.calls).toEqual(['holdAfterTurn L1']);
    expect(r.notice).toBe('session.state.pause_pending');
  });

  it('says the session is paused when the daemon held it at once', async () => {
    const api = fakeApi({ action: 'held' });
    const r = await runSessionAction(api, 'L1', 'pause');
    expect(r.notice).toBe('chat.paused');
  });

  it('cancels a queued pause through the daemon', async () => {
    const api = fakeApi();
    const r = await runSessionAction(api, 'L1', 'cancelPause');
    expect(api.calls).toEqual(['cancelHold L1']);
    expect(r.notice).toBe('chat.pauseCancelled');
  });

  it('interrupts with Escape, closes with confirmation and resumes into a terminal', async () => {
    const api = fakeApi();
    await runSessionAction(api, 'L1', 'interrupt');
    const closed = await runSessionAction(api, 'L1', 'close');
    const resumed = await runSessionAction(api, 'L1', 'resume');
    expect(api.calls).toEqual(['keys L1 esc', 'close L1 true', 'resume L1']);
    expect(closed).toEqual({ notice: 'chat.closed', dropChat: true });
    expect(resumed.term).toEqual({ id: 't1' });
  });

  it('lets a daemon refusal reach the caller', async () => {
    const api = fakeApi();
    api.holdAfterTurn = async () => Promise.reject(new Error('refused'));
    await expect(runSessionAction(api, 'L1', 'pause')).rejects.toThrow('refused');
  });
});
