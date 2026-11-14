import { describe, expect, it } from 'vitest';
import { quietMock } from '../../test/quiet-mock';
import { api } from './api';
import agentsSrc from '../../../../src/agents.rs?raw';

const daemonNote = () => {
  const src = agentsSrc;
  const m = /pub const CODEX_TRUST: &str = "((?:[^"\\]|\\.)*)";/.exec(src);
  if (!m) throw new Error('CODEX_TRUST not found in src/agents.rs');
  return m[1].replace(/\\"/g, '"');
};

describe('mock codex hook trust note', () => {
  it('is character for character the daemon CODEX_TRUST text', async () => {
    const st = await api(quietMock()).hooksStatus();
    const row = st.agents.find((r) => r.agent === 'codex');
    expect(row?.note).toBe(daemonNote());
  });
});
