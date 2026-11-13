import { describe, expect, it } from 'vitest';
import { api } from './api';
import type { DaemonClient } from './client';
import { feedbackApi } from '../feedback/feedback-api';
import { portsApi } from '../ports/ports-api';
import { systemApi } from '../system/api';
import { usageApi } from '../usage/api';

const capture = () => {
  const calls: unknown[][] = [];
  const c = { request: async (...a: unknown[]) => void calls.push(a) } as unknown as DaemonClient;
  return { calls, c };
};

describe('daemon routes spelled by the app', () => {
  it('asks for the usage of a range, grouped, with the timezone', async () => {
    const { calls, c } = capture();
    const a = usageApi(c);
    await a.usage('7d', 'project', 120);
    await a.usage('30d', 'session', -330, 'L1');
    await a.usage('today', 'agent', 0, undefined, true);
    expect(calls).toEqual([
      ['GET', '/v1/usage?range=7d&by=project&tzOffset=120'],
      ['GET', '/v1/usage?range=30d&by=session&tzOffset=-330&line=L1'],
      ['GET', '/v1/usage?range=today&by=agent&tzOffset=0&claudeLimits=1'],
    ]);
  });

  it('asks for Claude limits only when enabled', async () => {
    const { calls, c } = capture();
    await usageApi(c).usage('today', 'agent', 0);
    expect(String(calls[0][1])).not.toContain('claudeLimits');
  });

  it('tails the server log', async () => {
    const { calls, c } = capture();
    await api(c).logs(300);
    expect(calls).toEqual([['GET', '/v1/logs?tail=300']]);
  });

  it('polls the system monitor from the last sample', async () => {
    const { calls, c } = capture();
    await systemApi(c).system(null);
    await systemApi(c).system(1700);
    expect(calls).toEqual([['GET', '/v1/system'], ['GET', '/v1/system?since=1700']]);
  });

  it('reports the theme', async () => {
    const { calls, c } = capture();
    await api(c).reportTheme({ theme: 'dark' });
    expect(calls).toEqual([['POST', '/v1/theme', { theme: 'dark' }]]);
  });

  it('confirms a port kill with the process id', async () => {
    const { calls, c } = capture();
    await portsApi(c).killPort(5173, 4242);
    expect(calls).toEqual([['POST', '/v1/ports/5173/kill', { pid: 4242, confirm: true }]]);
  });

  it('starts and files feedback', async () => {
    const { calls, c } = capture();
    await feedbackApi(c).startFeedback({ agent: 'claude' });
    await feedbackApi(c).directFeedback({ repo: 'o/r' });
    expect(calls).toEqual([['POST', '/v1/feedback/start', { agent: 'claude' }], ['POST', '/v1/feedback/direct', { repo: 'o/r' }]]);
  });
});
