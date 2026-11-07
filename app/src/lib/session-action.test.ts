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
    // Given a session whose agent is mid-turn
    const api = fakeApi({ action: 'hold-pending' });
    // When the user clicks Pause
    const r = await runSessionAction(api, 'L1', 'pause');
    // Then the pause is queued after the turn, never sent as a plain hold
    expect(api.calls).toEqual(['holdAfterTurn L1']);
    expect(r.notice).toBe('session.state.pause_pending');
  });

  it('says the session is paused when the daemon held it at once', async () => {
    // Given an idle session
    const api = fakeApi({ action: 'held' });
    // When the user clicks Pause
    const r = await runSessionAction(api, 'L1', 'pause');
    // Then the toast says paused
    expect(r.notice).toBe('chat.paused');
  });

  it('cancels a queued pause through the daemon', async () => {
    // Given a session with a queued pause
    const api = fakeApi();
    // When the user clicks Cancel the queued pause
    const r = await runSessionAction(api, 'L1', 'cancelPause');
    // Then the hold is cancelled on the daemon and the user is told
    expect(api.calls).toEqual(['cancelHold L1']);
    expect(r.notice).toBe('chat.pauseCancelled');
  });

  it('interrupts with Escape, closes with confirmation and resumes into a terminal', async () => {
    // Given a session
    const api = fakeApi();
    // When the user interrupts, closes, then resumes
    await runSessionAction(api, 'L1', 'interrupt');
    const closed = await runSessionAction(api, 'L1', 'close');
    const resumed = await runSessionAction(api, 'L1', 'resume');
    // Then each action reaches its route
    expect(api.calls).toEqual(['keys L1 esc', 'close L1 true', 'resume L1']);
    expect(closed).toEqual({ notice: 'chat.closed', dropChat: true });
    expect(resumed.term).toEqual({ id: 't1' });
  });

  it('lets a daemon refusal reach the caller', async () => {
    // Given a daemon that refuses the hold
    const api = fakeApi();
    api.holdAfterTurn = async () => Promise.reject(new Error('refused'));
    // When the user clicks Pause
    // Then the error propagates so the view can show it
    await expect(runSessionAction(api, 'L1', 'pause')).rejects.toThrow('refused');
  });
});

describe('ChatView wiring', () => {
  it('runs every session bar action through runSessionAction, with no pending TODO', async () => {
    // Given the chat view source
    const src = (await import('./components/chat/ChatView.svelte?raw')).default as string;
    // When the session bar fires an action
    // Then the view delegates to runSessionAction and no longer calls a plain hold
    expect(src).toMatch(/await runSessionAction\(app\.api, lineId, kind\)/);
    expect(src).not.toMatch(/app\.api\.hold\(/);
    expect(src).not.toMatch(/TODO\((holdAfterTurn|cancelHold)\)/);
  });
});
