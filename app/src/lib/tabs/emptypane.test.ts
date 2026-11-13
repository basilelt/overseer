import { describe, expect, it } from 'vitest';
import { emptyPane, pickAgent } from './emptypane';
import type { AgentInfo, Workspace } from '../types';

const agent = (id: string, installed = true) => ({ id, name: id.toUpperCase(), installed }) as AgentInfo;
const AGENTS = [agent('claude'), agent('codex'), agent('amp', false), agent('shell')];
const ws = (over: Partial<Workspace> = {}) => ({ id: 'ws_a', name: 'alpha', path: '/home/u/alpha', ...over }) as Workspace;
const base = { sidebarMode: 'workspaces', workspaceId: 'ws_a', workspace: ws(), members: 0, agents: AGENTS, defaultAgent: 'claude', connected: true };

describe('pickAgent', () => {
  it('returns the default agent when it is installed', () => {
    expect(pickAgent(AGENTS, 'codex')?.id).toBe('codex');
  });

  it('falls back to the first installed agent when the default is not installed', () => {
    expect(pickAgent(AGENTS, 'amp')?.id).toBe('claude');
  });

  it('never picks the shell as the agent', () => {
    expect(pickAgent([agent('shell')], 'shell')).toBeNull();
  });
});

describe('emptyPane', () => {
  it('names the workspace and flags it empty when it has no sessions', () => {
    const v = emptyPane(base);
    expect(v.workspace).toEqual({ name: 'alpha', path: '/home/u/alpha', missing: false });
    expect(v.noSessions).toBe(true);
    expect(v.canStart).toBe(true);
    expect(v.agent?.id).toBe('claude');
  });

  it('is not empty when the workspace has sessions but none is open', () => {
    expect(emptyPane({ ...base, members: 2 }).noSessions).toBe(false);
  });

  it('blocks starting sessions when the workspace folder is missing', () => {
    const v = emptyPane({ ...base, workspace: ws({ missing: true }) });
    expect(v.workspace?.missing).toBe(true);
    expect(v.canStart).toBe(false);
  });

  it('blocks starting sessions while disconnected', () => {
    expect(emptyPane({ ...base, connected: false }).canStart).toBe(false);
  });

  it('shows no workspace in status mode', () => {
    const v = emptyPane({ ...base, sidebarMode: 'status' });
    expect(v.workspace).toBeNull();
    expect(v.noSessions).toBe(false);
  });

  it('shows no workspace for a group', () => {
    expect(emptyPane({ ...base, workspaceId: 'group:g1' }).workspace).toBeNull();
  });

  it('shows no workspace for sessions outside any workspace', () => {
    expect(emptyPane({ ...base, workspaceId: '', workspace: null }).workspace).toBeNull();
  });

  it('ignores a current workspace that is not the selected one', () => {
    expect(emptyPane({ ...base, workspaceId: 'ws_b' }).workspace).toBeNull();
  });
});
