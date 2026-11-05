import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setClient } from './client';
import { t } from './i18n.svelte';
import { MockDaemon } from './mock';
import { AGENT_WAIT_MS, type Proposal } from './organize';
import { organizer } from './organize.svelte';
import { settings } from './settings.svelte';
import { app, type ToastAction } from './state.svelte';
import type { ConnState, HostTarget } from './types';

class Daemon extends MockDaemon {
  calls: string[] = [];
  bodies: unknown[] = [];
  fail: Record<string, string> = {};

  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    onState({ phase: 'connected', target });
  }

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const key = `${method} ${path.split('?')[0]}`;
    if (key.includes('/organize/')) {
      this.calls.push(key);
      this.bodies.push(body);
    }
    const err = Object.entries(this.fail).find(([k]) => key.startsWith(k))?.[1];
    if (err) throw new Error(err);
    return super.request<T>(method, path, body);
  }
}

const local: HostTarget = { kind: 'local', label: 'Local' };

const proposal = (over: Partial<Proposal> = {}): Proposal => ({
  id: 'pr_x',
  version: 1,
  createdAt: '2026-10-30T12:00:00Z',
  status: 'pending_review',
  source: 'agent',
  summary: 's',
  actions: [],
  pickUp: [],
  reasons: {},
  ...over,
});

const reset = () => {
  organizer.close();
  Object.assign(organizer, { proposal: null, results: null, manual: false, late: false, busy: false, hidden: [] });
};

describe('organizer', () => {
  let m: Daemon;
  let notify: ReturnType<typeof vi.spyOn>;

  beforeEach(async () => {
    reset();
    m = new Daemon({ live: false, delayMs: 0 });
    setClient(m);
    app.client = m;
    await app.connect(local);
    notify = vi.spyOn(app, 'notify').mockImplementation(() => {});
  });

  afterEach(async () => {
    vi.useRealTimers();
    notify.mockRestore();
    await app.disconnect();
    settings.explicitlyDisconnected = false;
    reset();
  });

  it('opens the choice step with a clean slate', () => {
    organizer.late = true;
    organizer.manual = true;
    organizer.show();
    expect(organizer.phase).toBe('choice');
    expect(organizer.late).toBe(false);
    expect(organizer.manual).toBe(false);
  });

  describe('quick plan', () => {
    it('opens the deterministic proposal for review without a toast', async () => {
      organizer.show();
      organizer.hidden = ['old'];
      const run = organizer.quick();
      expect(organizer.busy).toBe(true);
      await run;
      expect(organizer.phase).toBe('review');
      expect(organizer.proposal?.source).toBe('deterministic');
      expect(organizer.proposal?.actions.length).toBeGreaterThan(0);
      expect(organizer.results).toBeNull();
      expect(organizer.hidden).toEqual([]);
      expect(organizer.busy).toBe(false);
      expect(notify).not.toHaveBeenCalled();
    });

    it('reports the daemon error and stays on the choice step', async () => {
      organizer.show();
      m.fail['POST /v1/organize/deterministic'] = 'nothing to organize';
      await organizer.quick();
      expect(notify).toHaveBeenCalledWith(t('org.failed', { error: 'nothing to organize' }), 'error');
      expect(organizer.phase).toBe('choice');
      expect(organizer.busy).toBe(false);
    });

    it('explains that it needs a connection', async () => {
      await app.disconnect();
      await organizer.quick();
      expect(notify).toHaveBeenCalledWith(t('org.failed', { error: t('org.offline') }), 'error');
      expect(m.calls).toEqual([]);
    });
  });

  describe('with an agent', () => {
    it('waits for the agent task and flags a configured MCP server', async () => {
      await organizer.withAgent({ agent: 'claude', model: 'opus' });
      expect(m.bodies).toEqual([{ agent: 'claude', model: 'opus' }]);
      expect(organizer.phase).toBe('waiting');
      expect(organizer.waitingFor).toMatch(/.+/);
      expect(organizer.manual).toBe(false);
      expect(organizer.busy).toBe(false);
    });

    it('warns when the agent needs the MCP server set up by hand', async () => {
      await organizer.withAgent({ agent: 'codex' });
      expect(organizer.manual).toBe(true);
    });

    it('marks the wait as late only after the agent wait budget', async () => {
      vi.useFakeTimers();
      await organizer.withAgent({ agent: 'claude' });
      await vi.advanceTimersByTimeAsync(AGENT_WAIT_MS - 1);
      expect(organizer.late).toBe(false);
      await vi.advanceTimersByTimeAsync(1);
      expect(organizer.late).toBe(true);
    });

    it('restarts the late timer when asked again', async () => {
      vi.useFakeTimers();
      await organizer.withAgent({ agent: 'claude' });
      await vi.advanceTimersByTimeAsync(AGENT_WAIT_MS - 10);
      await organizer.withAgent({ agent: 'claude' });
      await vi.advanceTimersByTimeAsync(20);
      expect(organizer.late).toBe(false);
      await vi.advanceTimersByTimeAsync(AGENT_WAIT_MS);
      expect(organizer.late).toBe(true);
    });

    it('opens the plan of the awaited task as soon as it arrives', async () => {
      vi.useFakeTimers();
      await organizer.withAgent({ agent: 'claude' });
      const taskId = organizer.waitingFor!;
      m.emit('organize.proposal', { proposal: proposal({ taskId, results: [] }) });
      expect(organizer.phase).toBe('review');
      expect(organizer.proposal?.taskId).toBe(taskId);
      expect(organizer.results).toEqual([]);
      expect(organizer.waitingFor).toBeNull();
      expect(notify).not.toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(AGENT_WAIT_MS * 2);
      expect(organizer.late).toBe(false);
    });

    it('offers a plan from another task in a toast that opens it', async () => {
      organizer.show();
      m.emit('organize.proposal', { proposal: proposal({ taskId: 'tk_other' }) });
      expect(organizer.phase).toBe('choice');
      expect(notify).toHaveBeenCalledWith(t('org.ready'), 'info', expect.objectContaining({ label: t('org.review') }));
      const action = notify.mock.calls[0][2] as ToastAction;
      action.run();
      expect(organizer.phase).toBe('review');
      expect(organizer.proposal?.taskId).toBe('tk_other');
    });

    it('ignores deterministic and already handled proposals', () => {
      organizer.show();
      m.emit('organize.proposal', { proposal: proposal({ source: 'deterministic' }) });
      m.emit('organize.proposal', { proposal: proposal({ status: 'applied' }) });
      m.emit('organize.proposal', { proposal: proposal({ status: 'dismissed' }) });
      expect(notify).not.toHaveBeenCalled();
      expect(organizer.phase).toBe('choice');
    });

    it('reports a failed start and does not wait', async () => {
      m.fail['POST /v1/organize/start'] = 'no agent';
      organizer.show();
      await organizer.withAgent({ agent: 'claude' });
      expect(notify).toHaveBeenCalledWith(t('org.failed', { error: 'no agent' }), 'error');
      expect(organizer.phase).toBe('choice');
      expect(organizer.waitingFor).toBeNull();
      expect(organizer.busy).toBe(false);
    });
  });

  describe('apply', () => {
    it('applies the chosen actions and records the results', async () => {
      await organizer.quick();
      const p = organizer.proposal!;
      const pin = p.actions.find((a) => a.kind === 'pin')!;
      m.calls = [];
      await organizer.apply([pin.id]);
      expect(m.calls).toEqual([`POST /v1/organize/proposals/${p.id}/apply`]);
      expect(m.bodies.at(-1)).toEqual({ actionIds: [pin.id] });
      expect(organizer.results).toEqual([expect.objectContaining({ actionId: pin.id, kind: 'pin', status: 'done' })]);
      expect(organizer.proposal).toMatchObject({ id: p.id, status: 'applied', results: organizer.results });
      expect(organizer.busy).toBe(false);
    });

    it('does nothing without a proposal or while busy', async () => {
      await organizer.apply(['a1']);
      organizer.open(proposal());
      organizer.busy = true;
      await organizer.apply(['a1']);
      expect(m.calls).toEqual([]);
      expect(organizer.busy).toBe(true);
    });

    it('keeps the proposal pending when applying fails', async () => {
      organizer.open(proposal({ id: 'pr_missing' }));
      await organizer.apply(['a1']);
      expect(notify).toHaveBeenCalledWith(expect.stringContaining('pr_missing'), 'error');
      expect(organizer.proposal?.status).toBe('pending_review');
      expect(organizer.results).toBeNull();
      expect(organizer.busy).toBe(false);
    });
  });

  describe('dismiss and close', () => {
    it('dismisses a pending proposal on the daemon and closes', async () => {
      await organizer.quick();
      const id = organizer.proposal!.id;
      m.calls = [];
      await organizer.dismiss();
      expect(organizer.phase).toBeNull();
      expect(m.calls).toEqual([`DELETE /v1/organize/proposals/${id}`]);
    });

    it('only closes once the proposal was applied', async () => {
      organizer.open(proposal({ status: 'applied' }));
      await organizer.dismiss();
      expect(organizer.phase).toBeNull();
      expect(m.calls).toEqual([]);
    });

    it('swallows a failed dismiss', async () => {
      organizer.open(proposal({ id: 'pr_missing' }));
      await expect(organizer.dismiss()).resolves.toBeUndefined();
      expect(organizer.phase).toBeNull();
    });

    it('closes a pending plan offline without a rejected promise', async () => {
      organizer.open(proposal());
      await app.disconnect();
      await expect(organizer.dismiss()).resolves.toBeUndefined();
      expect(organizer.phase).toBeNull();
    });

    it('closes without a request when nothing is open', async () => {
      await organizer.dismiss();
      expect(m.calls).toEqual([]);
    });

    it('stops waiting for the agent on close', async () => {
      vi.useFakeTimers();
      await organizer.withAgent({ agent: 'claude' });
      organizer.close();
      expect(organizer.phase).toBeNull();
      expect(organizer.waitingFor).toBeNull();
      await vi.advanceTimersByTimeAsync(AGENT_WAIT_MS);
      expect(organizer.late).toBe(false);
    });
  });

  it('moves to the pick-up step and hides reviewed lines one by one', () => {
    organizer.open(proposal());
    organizer.pickUp();
    organizer.hide('ln_01');
    organizer.hide('ln_02');
    expect(organizer.phase).toBe('pickup');
    expect(organizer.hidden).toEqual(['ln_01', 'ln_02']);
  });
});
