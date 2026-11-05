import { describe, expect, it } from 'vitest';
import { hooksOffer, outcome } from './hooks';
import type { ConnState, HookAgentStatus, HooksStatus } from './types';

const row = (agent: string, p: Partial<HookAgentStatus>): HookAgentStatus => ({
  agent,
  name: agent.toUpperCase(),
  mechanism: 'hooks',
  cliInstalled: true,
  configured: true,
  installed: false,
  current: false,
  ...p,
});
const status = (agents: HookAgentStatus[]): HooksStatus => ({ shim: '/s', agents });
const conn: ConnState = { phase: 'connected', target: { kind: 'ssh', host: 'box', label: 'box' } };

describe('hooks offer', () => {
  it('offers the configured agents that lack hooks', () => {
    const s = status([row('claude', {}), row('codex', { installed: true }), row('gemini', { configured: false }), row('agy', { mechanism: 'polling' }), row('opencode', { mechanism: 'plugin' })]);
    expect(hooksOffer(s, conn, [], [])?.map((a) => a.agent)).toEqual(['claude', 'opencode']);
  });

  it('stays quiet when declined, disconnected or complete', () => {
    const s = status([row('claude', {})]);
    expect(hooksOffer(s, conn, ['ssh:box'], [])).toBeNull();
    expect(hooksOffer(s, conn, [], ['ssh:box'])).toBeNull();
    expect(hooksOffer(s, { ...conn, phase: 'reconnecting' }, [], [])).toBeNull();
    expect(hooksOffer(status([row('claude', { installed: true })]), conn, [], [])).toBeNull();
    expect(hooksOffer(null, conn, [], [])).toBeNull();
  });

  it('summarizes an install and flags the Codex trust step', () => {
    const o = outcome(
      {
        dryRun: false,
        results: [
          { agent: 'claude', path: 'a', changed: true, written: true, diff: '' },
          { agent: 'codex', path: 'b', changed: true, written: true, diff: '', note: 'trust' },
          { agent: 'gemini', path: 'c', changed: false, written: false, diff: '', skipped: 'not configured' },
          { agent: 'cursor', path: 'd', changed: true, written: false, diff: '', error: 'bad json' },
        ],
      },
      (id) => id.toUpperCase(),
    );
    expect(o).toEqual({ done: ['CLAUDE', 'CODEX'], failed: [{ agent: 'CURSOR', error: 'bad json' }], codexTrust: true });
  });
});
