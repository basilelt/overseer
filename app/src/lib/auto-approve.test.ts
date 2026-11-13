import { describe, expect, it, vi } from 'vitest';
import { api } from './daemon/api';
import { allState, setAll } from './auto-approve';
import { quietMock } from '../test/quiet-mock';

const fresh = () => api(quietMock());

describe('allState', () => {
  it('is on when every listed agent auto-approves', () => {
    expect(allState({ claude: true, codex: true }, ['claude', 'codex'])).toBe('on');
  });

  it('is off when no listed agent auto-approves', () => {
    expect(allState({ claude: false, codex: false }, ['claude', 'codex'])).toBe('off');
  });

  it('is mixed when some agents auto-approve and others do not', () => {
    expect(allState({ claude: true, codex: false }, ['claude', 'codex'])).toBe('mixed');
  });

  it('ignores agents the host does not configure', () => {
    expect(allState({ claude: false }, ['claude', 'amp'])).toBe('off');
  });
});

describe('setAll', () => {
  it('turns auto approval off for every agent on the daemon', async () => {
    const a = fresh();
    const { autoApprove } = await a.agentSettings();
    const ids = Object.keys(autoApprove);
    const next = await setAll(a, autoApprove, ids, false);
    expect(Object.values(next).every((v) => v === false)).toBe(true);
    expect((await a.agentSettings()).autoApprove).toEqual(next);
  });

  it('turns it back on and only writes the agents that differ', async () => {
    const a = fresh();
    const ids = Object.keys((await a.agentSettings()).autoApprove);
    const off = await setAll(a, (await a.agentSettings()).autoApprove, ids, false);
    const mixed = (await a.setAgentAutoApprove('codex', true)).autoApprove;
    expect(allState(mixed, ids)).toBe('mixed');
    const spy = vi.spyOn(a, 'setAgentAutoApprove');
    const on = await setAll(a, mixed, ids, true);
    expect(spy).toHaveBeenCalledTimes(ids.length - 1);
    expect(spy.mock.calls.map((c) => c[0])).not.toContain('codex');
    expect(allState(on, ids)).toBe('on');
    expect(off).not.toEqual(on);
  });

  it('skips agents the host does not configure', async () => {
    const a = fresh();
    const { autoApprove } = await a.agentSettings();
    const next = await setAll(a, autoApprove, ['claude', 'amp', 'shell'], false);
    expect(next.claude).toBe(false);
    expect(next.codex).toBe(true);
  });

  it('reports the failure with the state read back from the daemon', async () => {
    const a = fresh();
    const { autoApprove } = await a.agentSettings();
    const ids = Object.keys(autoApprove);
    const real = a.setAgentAutoApprove;
    let calls = 0;
    a.setAgentAutoApprove = (agent, on) => (++calls === 2 ? Promise.reject(new Error('daemon unreachable')) : real(agent, on));
    const err = await setAll(a, autoApprove, ids, false).catch((e) => e);
    expect(err.message).toBe('daemon unreachable');
    expect(err.autoApprove).toEqual((await a.agentSettings()).autoApprove);
    expect(err.autoApprove[ids[0]]).toBe(false);
    expect(err.autoApprove[ids[1]]).toBe(true);
  });

  it('falls back to the last known state when the daemon cannot be read back', async () => {
    const a = fresh();
    const { autoApprove } = await a.agentSettings();
    a.setAgentAutoApprove = () => Promise.reject('offline');
    a.agentSettings = () => Promise.reject(new Error('offline'));
    const err = await setAll(a, autoApprove, ['claude'], false).catch((e) => e);
    expect(err).toBeInstanceOf(Error);
    expect(err.message).toBe('offline');
    expect(err.autoApprove).toEqual(autoApprove);
  });
});
