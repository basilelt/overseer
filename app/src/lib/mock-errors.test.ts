import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { api } from './api';
import { DAEMON_ERRORS, MockDaemon } from './mock';

const setup = () => {
  const m = new MockDaemon({ live: false, delayMs: 0 });
  return { m, a: api(m) };
};

const literal = (text: string) => text.replace(/\\/g, '\\\\').replace(/"/g, '\\"');

describe('the strings the mock copies from the daemon', () => {
  it.each(Object.entries(DAEMON_ERRORS))('%s is still written in the Rust source', (_k, e) => {
    const text = typeof e.text === 'function' ? e.text('{id}') : e.text;
    const needle = literal(text);
    const files = e.source.split(', ').map((f) => readFileSync(resolve(process.cwd(), '..', f), 'utf8'));
    expect(files.some((src) => src.includes(needle))).toBe(true);
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
