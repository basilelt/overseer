import { beforeAll, describe, expect, it } from 'vitest';
import { api } from './api';
import { DAEMON_ERRORS, MockDaemon } from './mock';
import { quietMock } from '../../test/quiet-mock';

interface Fs {
  readFileSync(path: URL, enc: 'utf8'): string;
}
let fs: Fs;
beforeAll(async () => {
  const name: string = 'node:fs';
  fs = await import(/* @vite-ignore */ name);
});

const setup = () => {
  const m = quietMock();
  return { m, a: api(m) };
};

const goldenFile = (): Record<string, { status: number; message: string }> =>
  JSON.parse(fs.readFileSync(new URL('../../../../tests/golden/daemon_errors.json', import.meta.url), 'utf8'));

const sessionTask = async (a: ReturnType<typeof api>) => (await a.tasks()).find((t) => t.origin === 'session')!;
const quorumBase = { title: 'Q', description: '', project: '/p', model: '' };

const triggers: Record<string, (m: MockDaemon, a: ReturnType<typeof api>) => Promise<unknown>> = {
  noTerm: (m) => m.request('POST', '/v1/terms/nope/kill', { confirm: true }),
  termStillRunning: (m) => m.request('DELETE', '/v1/terms/tm_shell1'),
  termExited: async (m) => {
    await m.request('POST', '/v1/terms/tm_shell1/kill', { confirm: true });
    return m.request('POST', '/v1/terms/tm_shell1/kill', { confirm: true });
  },
  killNeedsConfirm: (m) => m.request('POST', '/v1/terms/tm_shell1/kill', {}),
  termSize: (m) => m.request('POST', '/v1/terms/tm_shell1/resize', { cols: 0, rows: 20 }),
  termSizeRequired: (m) => m.request('POST', '/v1/terms/tm_shell1/resize', {}),
  shellInQuorum: (_m, a) => a.createTask({ ...quorumBase, agent: 'claude', quorum: { workers: ['shell'], reviewers: ['codex'] } } as never),
  quorumOrchestrator: (_m, a) => a.createTask({ ...quorumBase, agent: 'gemini', quorum: { workers: ['claude'], reviewers: ['codex'] } } as never),
  sessionTaskStatus: async (_m, a) => a.patchTask((await sessionTask(a)).id, { status: 'pending' }),
};

describe('the mock daemon matches tests/golden/daemon_errors.json, which the real daemon is tested against', () => {
  it('lists the same entries as the golden', () => {
    expect(Object.keys(DAEMON_ERRORS).sort()).toEqual(Object.keys(goldenFile()).sort());
    expect(Object.keys(triggers).sort()).toEqual(Object.keys(goldenFile()).sort());
  });

  it.each(Object.keys(DAEMON_ERRORS))('%s has the golden text', (key) => {
    const e = DAEMON_ERRORS[key as keyof typeof DAEMON_ERRORS];
    const text = typeof e.text === 'function' ? e.text('{id}') : e.text;
    expect(text).toBe(goldenFile()[key].message);
  });

  it.each(Object.keys(DAEMON_ERRORS))('%s is returned with the golden status and message', async (key) => {
    const { m, a } = setup();
    const g = goldenFile()[key];
    await expect(triggers[key](m, a)).rejects.toMatchObject({ status: g.status, message: g.message.replace('{id}', 'nope') });
  });
});

describe('MockDaemon error strings', () => {
  it('names an unknown term like the daemon', async () => {
    const { m } = setup();
    await expect(m.request('POST', '/v1/terms/nope/kill', { confirm: true })).rejects.toMatchObject({ status: 404, message: 'no term nope' });
    await expect(m.request('DELETE', '/v1/terms/nope')).rejects.toMatchObject({ status: 404, message: 'no term nope' });
  });

  it('refuses to delete a running term', async () => {
    const { a } = setup();
    await expect(a.deleteTerm('tm_shell1')).rejects.toMatchObject({ status: 409, message: DAEMON_ERRORS.termStillRunning.text });
  });

  it('asks for the exact confirmation to kill a term', async () => {
    const { m } = setup();
    for (const body of [{}, { confirm: 'yes' }, { confirm: 1 }]) {
      await expect(m.request('POST', '/v1/terms/tm_shell1/kill', body)).rejects.toMatchObject({ status: 400, message: 'kill needs {"confirm":true}' });
    }
  });

  it('refuses to kill a term that has exited', async () => {
    const { m, a } = setup();
    await a.killTerm('tm_shell1');
    await expect(a.killTerm('tm_shell1')).rejects.toMatchObject({ status: 409, message: 'term has exited' });
  });

  it('refuses to resize an exited term or with a bad size', async () => {
    const { m, a } = setup();
    await expect(m.request('POST', '/v1/terms/tm_shell1/resize', {})).rejects.toMatchObject({ status: 400, message: 'cols and rows required' });
    await expect(m.request('POST', '/v1/terms/tm_shell1/resize', { cols: 0, rows: 20 })).rejects.toMatchObject({ status: 400, message: 'cols and rows must be integers in 1..=1000' });
    await expect(m.request('POST', '/v1/terms/tm_shell1/resize', { cols: 80 })).rejects.toMatchObject({ status: 400, message: 'cols and rows must be integers in 1..=1000' });
    expect(await m.request('POST', '/v1/terms/tm_shell1/resize', { cols: 100, rows: 30 })).toMatchObject({ cols: 100, rows: 30 });
    await a.killTerm('tm_shell1');
    await expect(m.request('POST', '/v1/terms/tm_shell1/resize', { cols: 100, rows: 30 })).rejects.toMatchObject({ status: 409, message: 'term has exited' });
  });

  it('keeps shell out of a quorum and requires a claude or codex orchestrator', async () => {
    const { a } = setup();
    const base = { title: 'Q', description: '', project: '/p', model: '' };
    await expect(a.createTask({ ...base, agent: 'claude', quorum: { workers: ['shell'], reviewers: ['codex'] } } as never)).rejects.toMatchObject({ status: 400, message: 'shell cannot join a quorum' });
    await expect(a.createTask({ ...base, agent: 'gemini', quorum: { workers: ['claude'], reviewers: ['codex'] } } as never)).rejects.toMatchObject({ status: 400, message: 'the quorum orchestrator must be claude or codex' });
  });

  it('limits the status of a session task', async () => {
    const { a } = setup();
    const session = (await a.tasks()).find((t) => t.origin === 'session')!;
    await expect(a.patchTask(session.id, { status: 'pending' })).rejects.toMatchObject({ status: 400, message: 'a session task follows its session: status must be one of in_progress, in_testing, merged, completed' });
  });

  it('returns the wrapped task from a review, like the daemon', async () => {
    const { m, a } = setup();
    const t = (await a.tasks()).find((x) => x.origin !== 'session' && x.status === 'in_testing')!;
    const raw = (await m.request('POST', `/v1/tasks/${t.id}/review`, { action: 'approve' })) as { task: { id: string } };
    expect(raw.task.id).toBe(t.id);
  });
});
