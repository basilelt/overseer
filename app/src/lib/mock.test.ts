import { describe, expect, it, vi } from 'vitest';
import { api } from './api';
import { inject, MockDaemon } from './mock';
import { applyEvent, type Snapshot } from './reduce';
import { tabApi } from './tab-edits';
import { sourcesApi } from './task-sources';
import type { ConnPhase, DaemonEvent } from './types';

function setup() {
  const m = new MockDaemon({ live: false, delayMs: 0 });
  const events: DaemonEvent[] = [];
  m.subscribe((e) => events.push(e));
  return { m, a: { ...api(m), ...tabApi(m) }, events };
}

describe('MockDaemon', () => {
  it('serves a realistic snapshot', async () => {
    const { a } = setup();
    const [lines, tasks, terms, projects] = await Promise.all([a.lines(), a.tasks(), a.terms(), a.projects()]);
    expect(lines.length).toBe(12);
    expect(tasks.length).toBeGreaterThanOrEqual(12);
    expect(new Set(tasks.map((t) => t.status)).size).toBe(6);
    expect(terms.some((t) => !t.lineId)).toBe(true);
    expect(projects.length).toBeGreaterThan(2);
  });

  it('accepts merged on a task and rejects a status the daemon does not know', async () => {
    const { a } = setup();
    const tasks = await a.tasks();
    const plain = tasks.find((t) => t.origin !== 'session' && t.status === 'in_testing')!;
    expect((await a.patchTask(plain.id, { status: 'merged' })).status).toBe('merged');
    await expect(a.patchTask(plain.id, { status: 'done' as never })).rejects.toMatchObject({ status: 400, message: 'status must be one of pending, auto, in_testing, merged, completed' });
    const session = tasks.find((t) => t.origin === 'session')!;
    expect((await a.patchTask(session.id, { status: 'merged' })).status).toBe('merged');
    await expect(a.patchTask(session.id, { status: 'pending' })).rejects.toMatchObject({ status: 400, message: 'a session task follows its session: status must be one of in_progress, in_testing, merged, completed' });
  });

  it('moves a task back to in_progress only while its agent runs, as the daemon does', async () => {
    const { a } = setup();
    const tasks = await a.tasks();
    const idle = tasks.find((t) => t.origin !== 'session' && t.status === 'pending')!;
    await expect(a.patchTask(idle.id, { status: 'in_progress' })).rejects.toMatchObject({ status: 400, message: 'use POST /v1/tasks/{id}/start to start a task' });
    const live = tasks.find((t) => t.origin !== 'session' && t.status === 'in_progress' && t.termId)!;
    await a.patchTask(live.id, { status: 'merged' });
    expect((await a.patchTask(live.id, { status: 'in_progress' })).status).toBe('in_progress');
  });

  it('reports the device it was given, for the demo on a real phone', async () => {
    expect(await new MockDaemon({ live: false, platform: { os: 'ios', mobile: true } }).platform()).toEqual({ os: 'ios', mobile: true });
    expect((await setup().m.platform()).mobile).toBe(false);
  });

  it('browses host folders through the dirs route', async () => {
    const { a } = setup();
    const home = await a.dirs();
    expect(home.path).toBe(home.home);
    expect(home.dirs.some((d) => d.name.startsWith('.'))).toBe(false);
    expect((await a.dirs('~', true)).dirs[0].name).toBe('.cache');
    const work = await a.dirs('~/Documents/Github/work/');
    expect(work.parent).toBe(`${home.home}/Documents/Github`);
    expect(work.dirs.find((d) => d.name === 'webshop')).toMatchObject({ path: `${home.home}/Documents/Github/work/webshop`, git: true });
    expect((await a.dirs('/')).parent).toBeNull();
    await expect(a.dirs('~/nope')).rejects.toMatchObject({ status: 404 });
    await expect(a.dirs('rel')).rejects.toMatchObject({ status: 400 });
  });

  it('lists, adds and removes task sources per project and serves their issues', async () => {
    const { m, a } = setup();
    const src = sourcesApi(m);
    const projects = await a.projects();
    const sb = projects.find((p) => p.name === 'overseer')!;
    const other = projects.find((p) => p.name === 'webshop')!;
    expect(await src.list(sb.path)).toEqual([expect.objectContaining({ kind: 'github', implicit: true })]);
    const jira = await src.add(sb.path, 'jira', ' project = AS ');
    expect(jira).toMatchObject({ kind: 'jira', target: 'project = AS', implicit: false });
    await expect(src.add(sb.path, 'jira', 'project = AS')).rejects.toMatchObject({ status: 409 });
    await expect(src.add(sb.path, 'github', 'nope')).rejects.toMatchObject({ status: 400 });
    await expect(src.add('rel', 'jira', 'x')).rejects.toMatchObject({ status: 400 });
    expect((await src.list(other.path)).every((s) => s.implicit)).toBe(true);
    const got = await src.items(sb.path);
    expect(got.sources.map((s) => s.kind)).toEqual(['github', 'jira']);
    expect(got.items.some((i) => i.key === '#50')).toBe(true);
    expect(got.items.find((i) => i.key === 'PROJ-1204')?.ticketKey).toBe('PROJ-1204');
    expect(await src.add(sb.path, 'gitlab', 'grp/sub/widget')).toMatchObject({ kind: 'gitlab' });
    await expect(src.add(sb.path, 'gitlab', 'widget')).rejects.toMatchObject({ status: 400 });
    expect((await src.items(sb.path)).items.find((i) => i.kind === 'gitlab')?.url).toContain('grp/sub/widget');
    await src.add(sb.path, 'github', 'acme/fail');
    expect((await src.items(sb.path)).errors).toEqual([expect.objectContaining({ error: expect.stringContaining('acme/fail') })]);
    await src.remove(jira.id);
    await expect(src.remove(jira.id)).rejects.toMatchObject({ status: 404 });
    expect((await src.list(sb.path)).some((s) => s.kind === 'jira')).toBe(false);
  });

  it('creates, starts and reviews a task with events', async () => {
    const { a, events } = setup();
    const t = await a.createTask({ title: ' Ship it ', description: '', project: 'infra', agent: 'claude', model: 'opus', effort: 'high' });
    expect(t).toMatchObject({ title: 'Ship it', status: 'pending' });
    const started = await a.startTask(t.id);
    expect(started.status).toBe('in_progress');
    expect(started.termId).toBeTruthy();
    const done = await a.reviewTask(t.id, 'commit');
    expect(done.status).toBe('completed');
    expect(events.map((e) => e.type)).toEqual(['task.created', 'term.created', 'task.updated', 'task.updated']);
  });

  it('moves task attachments out of the uploads and lists them in the description', async () => {
    const { a, m } = setup();
    const shot = await m.uploadFile('/Users/me/Screen Shot.png');
    const notes = await m.uploadFile('/Users/me/notes.txt');
    const t = await a.createTask({ title: 'Look', description: 'See files\n', project: '/p', agent: 'claude', model: '', attachments: [{ path: shot.path, name: 'Screen Shot.png' }, { path: notes.path }] });
    const file = notes.path.split('/').pop();
    const dir = `${notes.path.split('/.overseer/')[0]}/.overseer/tasks/${t.id}`;
    expect(t.description).toBe(`See files\n\nAttachments:\n- ${dir}/Screen_Shot.png\n- ${dir}/${file}`);
    await expect(a.createTask({ title: 'Again', description: '', project: '/p', agent: 'claude', model: '', attachments: [{ path: shot.path }] })).rejects.toThrow('no such upload');
  });

  it('refuses task attachments that are not uploads', async () => {
    const { a } = setup();
    const make = (attachments: unknown) => a.createTask({ title: 'x', description: '', project: '/p', agent: 'claude', model: '', attachments: attachments as never });
    await expect(make([{ path: '/etc/passwd' }])).rejects.toMatchObject({ status: 400, message: expect.stringContaining('is not an upload') });
    await expect(make('x')).rejects.toThrow('attachments must be a list');
    await expect(make([{ name: 'a' }])).rejects.toThrow('attachment path required');
    await expect(make(Array(21).fill({ path: '/x' }))).rejects.toThrow('at most 20 attachments');
  });

  it('mirrors the daemon on feedback attachments: only supported types, only uploads', async () => {
    const { m } = setup();
    const send = (path: string, attachments: unknown) => m.request('POST', path, { repo: 'o/r', title: 't', body: 'b', agent: 'claude', prompt: '@REPORT@', attachments });
    const shot = await m.uploadFile('/Users/me/shot.png');
    const notes = await m.uploadFile('/Users/me/notes.txt');
    for (const path of ['/v1/feedback/direct', '/v1/feedback/start']) {
      await expect(send(path, [{ path: shot.path, name: shot.name }])).resolves.toBeTruthy();
      await expect(send(path, [{ path: notes.path, name: notes.name }])).rejects.toMatchObject({ status: 400, message: 'notes.txt: GitHub only accepts png, jpg, jpeg, gif, webp, svg and mp4, mov, webm attachments' });
      await expect(send(path, [{ path: '/etc/passwd' }])).rejects.toMatchObject({ status: 400, message: '/etc/passwd is not an upload' });
      await expect(send(path, 'x')).rejects.toThrow('attachments must be a list');
    }
  });

  it('asks a running agent to approve and merge its PR on approve, like the daemon', async () => {
    const { a } = setup();
    const t = await a.createTask({ title: 'PR', description: '', project: 'infra', agent: 'claude', model: 'opus', effort: 'high' });
    await expect(a.reviewTask(t.id, 'approve', undefined, { merge: true })).rejects.toMatchObject({ status: 409, message: expect.stringContaining('the agent is not running') });
    expect((await a.tasks()).find((x) => x.id === t.id)?.status).toBe('pending');
    await a.startTask(t.id);
    expect((await a.reviewTask(t.id, 'approve', undefined, { merge: true, approvePr: true })).status).toBe('completed');
  });

  it('answers the protocol version', async () => {
    const { a } = setup();
    expect((await a.version()).protocol).toBe(1);
  });

  it('rejects a task without a title', async () => {
    const { a } = setup();
    await expect(a.createTask({ title: ' ', description: '', project: 'x', agent: 'codex', model: 'm', effort: 'low' })).rejects.toThrow('title');
  });

  it('holds a line and resumes it into a term', async () => {
    const { a, events } = setup();
    await a.hold('ln_10');
    expect((await a.lines()).find((l) => l.line.id === 'ln_10')!.line.state).toBe('held');
    const term = await a.resume('ln_11');
    expect(term.lineId).toBe('ln_11');
    expect(events.map((e) => e.type)).toContain('line.state_changed');
  });

  it('refuses to hold or close a busy line unless the close is forced, like the daemon', async () => {
    const { m, a } = setup();
    await expect(a.hold('ln_06')).rejects.toThrow('HTTP 409');
    await expect(a.close('ln_06')).rejects.toThrow('HTTP 409');
    await m.request('POST', '/v1/lines/ln_06/close?dryRun=false&force=true');
    expect((await a.lines()).find((l) => l.line.id === 'ln_06')!.line.state).toBe('hung-up');
  });

  it('ignores force on hold and answers 409 for a busy line, like the daemon', async () => {
    // Given a busy line
    // When it is held with force=true
    // Then the daemon ignores force: 409 with the failed atPrompt predicate, and the line stays active
    const { m, a } = setup();
    await expect(m.request('POST', '/v1/lines/ln_06/hold?dryRun=false&force=true')).rejects.toMatchObject({
      status: 409,
      body: { lineId: 'ln_06', action: 'none', predicates: { atPrompt: { ok: false, detail: 'agent is mid-turn' } } },
    });
    expect((await a.lines()).find((l) => l.line.id === 'ln_06')!.line.state).toBe('active');
  });

  it('refuses a plain hold of a pinned line or one waiting for input with 409, like the daemon', async () => {
    // Given an idle pinned line and a line that waits for the user
    // When each is held without afterTurn
    // Then both get 409 naming notPinned / notNeedsInput and stay active
    const { m, a } = setup();
    await a.pin('ln_09', true);
    await expect(a.hold('ln_09')).rejects.toMatchObject({ status: 409, body: { predicates: { notPinned: { ok: false, detail: 'line is pinned' } } } });
    await expect(a.hold('ln_01')).rejects.toMatchObject({ status: 409, body: { predicates: { notNeedsInput: { ok: false, detail: 'agent is waiting for you' } } } });
    await expect(m.request('POST', '/v1/lines/ln_09/hold')).rejects.toMatchObject({ status: 409 });
    const states = (await a.lines()).filter((l) => ['ln_09', 'ln_01'].includes(l.line.id)).map((l) => l.line.state);
    expect(states).toEqual(['active', 'idle']);
  });

  it('refuses a plain hold of a held or hung-up line with 409 on alive, like the daemon', async () => {
    // Given a held line and a line that was closed
    // When each is held
    // Then both fail the alive predicate with 409 and keep their state
    const { a } = setup();
    await a.close('ln_09');
    for (const id of ['ln_11', 'ln_09']) {
      await expect(a.hold(id)).rejects.toMatchObject({ status: 409, body: { predicates: { alive: { ok: false, detail: 'no live agent process for this line' } } } });
    }
    const states = (await a.lines()).filter((l) => ['ln_11', 'ln_09'].includes(l.line.id)).map((l) => l.line.state);
    expect(states).toEqual(['hung-up', 'held']);
  });

  it('answers a forced dry-run close of a busy line with 200 would-close, like the daemon', async () => {
    // Given a busy line
    // When a dry-run close is forced, and an unforced one is asked
    // Then the forced one plans would-close without touching it and the unforced one is 409
    const { m, a } = setup();
    const plan = (await m.request('POST', '/v1/lines/ln_06/close?force=true')) as { action: string };
    expect(plan.action).toBe('would-close');
    expect((await a.lines()).find((l) => l.line.id === 'ln_06')!.line.state).toBe('active');
    await expect(m.request('POST', '/v1/lines/ln_06/close')).rejects.toMatchObject({ status: 409 });
  });

  it('cancels a pending hold, and answers 409 once the hold is being applied, like the daemon', async () => {
    // Given a pending hold applied by the next tick
    // When it is cancelled before the tick, then again right after it
    // Then the first cancel succeeds and the second gets the daemon's 409
    const { m, a } = setup();
    (m.lines.find((l) => l.line.id === 'ln_09')!.local as { busy: boolean }).busy = true;
    expect((await a.holdAfterTurn('ln_09')).action).toBe('hold-pending');
    await expect(m.request('DELETE', '/v1/lines/ln_09/hold')).resolves.toMatchObject({ holdPending: false });
    expect((await a.holdAfterTurn('ln_09')).action).toBe('hold-pending');
    m.tick();
    m.tick();
    await expect(m.request('DELETE', '/v1/lines/ln_09/hold')).rejects.toMatchObject({ status: 409, message: expect.stringContaining('the hold is already being applied') });
  });

  it('exposes the running subagents on the line and keeps a pending hold while they run', async () => {
    // Given an idle line, and the same line with a subagent still running
    // When it is held after the turn, then the subagent stops
    // Then local.subagents is 0 or 1 like the daemon, the hold waits for the subagent and applies after
    const { m, a } = setup();
    const get = async () => (await a.lines()).find((l) => l.line.id === 'ln_09')!;
    expect((await get()).local?.subagents).toBe(0);
    (m.lines.find((l) => l.line.id === 'ln_09')!.local as { subagents: number }).subagents = 1;
    expect((await a.holdAfterTurn('ln_09')).action).toBe('hold-pending');
    m.tick();
    expect((await get()).local?.holdPending).toBe(true);
    (m.lines.find((l) => l.line.id === 'ln_09')!.local as { subagents: number }).subagents = 0;
    m.tick();
    expect((await get()).line.state).toBe('held');
  });

  it('queues an afterTurn hold with the noToolRunning predicate while a subagent runs, like the daemon', async () => {
    // Given an idle line with one running subagent
    // When it is held after the turn
    // Then the pending body names noToolRunning with the daemon's detail
    const { m, a } = setup();
    (m.lines.find((l) => l.line.id === 'ln_09')!.local as { subagents: number }).subagents = 1;
    const r = await a.holdAfterTurn('ln_09');
    expect(r.action).toBe('hold-pending');
    expect(r.predicates?.noToolRunning).toEqual({ ok: false, detail: 'a subagent is running' });
  });

  it('drops a pending hold blocked by a background task after the grace with line.hold_refused, like the daemon', async () => {
    // Given an idle line with a background task and a pending hold
    // When the mock ticks before and after the 10 s grace
    // Then the hold is dropped with a hold_refused event, and the line is never held
    vi.useFakeTimers();
    try {
      const { m, a, events } = setup();
      (m.lines.find((l) => l.line.id === 'ln_09')!.local as { tasks: number }).tasks = 1;
      expect((await a.holdAfterTurn('ln_09')).action).toBe('hold-pending');
      m.tick();
      vi.advanceTimersByTime(9_000);
      m.tick();
      const get = async () => (await a.lines()).find((l) => l.line.id === 'ln_09')!;
      expect((await get()).local?.holdPending).toBe(true);
      vi.advanceTimersByTime(1_000);
      m.tick();
      expect((await get()).local?.holdPending).toBeUndefined();
      expect((await get()).line.state).toBe('idle');
      const refused = events.filter((e) => (e.type as string) === 'line.hold_refused').map((e) => e.data);
      expect(refused).toEqual([{ lineId: 'ln_09', failed: [{ name: 'noToolRunning', detail: 'a background task is running' }], error: null }]);
    } finally {
      vi.useRealTimers();
    }
  });

  it('drops a pending hold of a line pinned after it was queued, never holding it, like the daemon', async () => {
    // Given an idle line with a pending hold that gets pinned afterwards
    // When the mock ticks before and after the 10 s grace
    // Then it is not held, and after the grace it is dropped with a hold_refused naming notPinned
    vi.useFakeTimers();
    try {
      const { m, a, events } = setup();
      (m.lines.find((l) => l.line.id === 'ln_09')!.local as { tasks: number }).tasks = 1;
      expect((await a.holdAfterTurn('ln_09')).action).toBe('hold-pending');
      await a.pin('ln_09', true);
      m.tick();
      vi.advanceTimersByTime(10_000);
      m.tick();
      const l = (await a.lines()).find((x) => x.line.id === 'ln_09')!;
      expect(l.line.state).not.toBe('held');
      expect(l.local?.holdPending).toBeUndefined();
      const refused = events.filter((e) => (e.type as string) === 'line.hold_refused').map((e) => e.data);
      expect(refused).toEqual([
        { lineId: 'ln_09', failed: [{ name: 'noToolRunning', detail: 'a background task is running' }, { name: 'notPinned', detail: 'line is pinned' }], error: null },
      ]);
    } finally {
      vi.useRealTimers();
    }
  });

  it('queues a hold on a busy line, applies it when the turn ends and stamps heldAt', async () => {
    // Given a busy line
    // When it is held with afterTurn, then the mock ticks twice
    // Then the turn ends first, the hold applies on the next tick with heldAt, and resume clears heldAt
    const { m, a } = setup();
    const get = async () => (await a.lines()).find((l) => l.line.id === 'ln_06')!;
    const r = await a.holdAfterTurn('ln_06');
    expect(r.action).toBe('hold-pending');
    expect((await get()).local?.holdPending).toBe(true);
    expect((await get()).line.state).toBe('active');
    m.tick();
    expect((await get()).local?.busy).toBe(false);
    expect((await get()).line.state).toBe('active');
    expect((await get()).local?.holdPending).toBe(true);
    m.tick();
    const held = await get();
    expect(held.line.state).toBe('held');
    expect(held.local?.holdPending).toBeUndefined();
    expect(Number.isNaN(Date.parse(held.local!.heldAt!))).toBe(false);
    await a.resume('ln_06');
    expect((await get()).local?.heldAt).toBeUndefined();
  });

  it('cancels a pending hold and answers an unknown line with 404 like the daemon', async () => {
    // Given a pending hold
    // When it is cancelled, then the turn ends
    // Then the line is never held; cancelling an unknown line is a 404
    const { m, a } = setup();
    await a.holdAfterTurn('ln_04');
    expect(await a.cancelHold('ln_04')).toEqual({ lineId: 'ln_04', holdPending: false });
    m.tick();
    const l = (await a.lines()).find((x) => x.line.id === 'ln_04')!;
    expect(l.line.state).toBe('active');
    expect(l.local?.holdPending).toBeUndefined();
    await expect(a.cancelHold('nope')).rejects.toMatchObject({ status: 404, message: expect.stringContaining("no line matches 'nope'") });
  });

  it('keeps a pending hold while the agent waits for the user and answers a dry run like the daemon', async () => {
    // Given a line that waits for the user
    // When a dry-run afterTurn hold is asked, then a real one, then the mock ticks
    // Then the dry run changes nothing and says would-hold-pending, and the hold stays pending
    const { m, a } = setup();
    const get = async () => (await a.lines()).find((l) => l.line.id === 'ln_01')!;
    const dry = await m.request('POST', '/v1/lines/ln_01/hold?afterTurn=true');
    expect((dry as { action: string }).action).toBe('would-hold-pending');
    expect((await get()).local?.holdPending).toBeUndefined();
    expect((await a.holdAfterTurn('ln_01')).action).toBe('hold-pending');
    m.tick();
    m.tick();
    expect((await get()).line.state).toBe('active');
    expect((await get()).local?.holdPending).toBe(true);
  });

  it('holds an idle line at once with afterTurn and refuses a pinned or stopped one', async () => {
    // Given an idle line, a pinned busy line and a held line
    // When each is held with afterTurn
    // Then the idle one is held, the others get 409 and nothing is pending
    const { a } = setup();
    const idle = (await a.lines()).find((l) => l.line.state === 'idle')!;
    expect((await a.holdAfterTurn(idle.line.id)).action).toBe('held');
    await expect(a.holdAfterTurn(idle.line.id)).rejects.toMatchObject({ status: 409 });
    await a.pin('ln_04', true);
    await expect(a.holdAfterTurn('ln_04')).rejects.toMatchObject({ status: 409 });
    expect((await a.lines()).find((l) => l.line.id === 'ln_04')!.local?.holdPending).toBeUndefined();
  });

  it('keeps a reducer snapshot in sync with its events', async () => {
    const { m, a, events } = setup();
    let s: Snapshot = { lines: await a.lines(), terms: await a.terms(), tasks: await a.tasks(), activity: {} };
    await a.deleteTask('tk_01');
    await a.patchTask('tk_02', { status: 'auto' });
    m.tick();
    for (const e of events) s = applyEvent(s, e);
    expect(s.tasks).toEqual(m.tasks);
    const act = events.find((e) => e.type === 'line.activity')!;
    expect(Object.keys(act.data)).toEqual(['lineId']);
    expect((await a.activity(act.data.lineId)).workLog.at(-1)!.state).toBe('running');
  });

  it('echoes terminal input and runs fake commands', async () => {
    const { m } = setup();
    let out = '';
    const dec = new TextDecoder();
    const io = await m.attachTerm('tm_shell1', (b) => (out += dec.decode(b)));
    io.write('pwd\r');
    expect(out).toContain('pwd\r\n/home/alice/Documents/Github/work/overseer');
  });

  it('lists every agent with install state and capabilities', async () => {
    const { a } = setup();
    const agents = await a.agents();
    expect(agents.map((x) => x.id)).toEqual(['claude', 'codex', 'gemini', 'agy', 'opencode', 'amp', 'copilot', 'cursor', 'shell']);
    expect(agents.filter((x) => !x.installed).map((x) => x.id)).toEqual(['opencode', 'amp', 'cursor']);
    expect(agents.find((x) => x.id === 'amp')!.install).toContain('npm');
    expect(agents.find((x) => x.id === 'gemini')!.efforts).toEqual([]);
    expect(agents.find((x) => x.id === 'agy')!.efforts).toEqual(['low', 'medium', 'high', 'xhigh', 'max']);
    expect(agents.filter((x) => x.sessions).map((x) => x.id)).toEqual(['claude', 'codex', 'gemini']);
  });

  it('persists per-agent auto-approval defaults and applies them to newly started terms', async () => {
    const { m, a } = setup();
    expect((await a.agentSettings()).autoApprove).toEqual({ claude: true, codex: true, gemini: true, agy: true, opencode: true, copilot: true, cursor: true });
    expect((await a.createTerm({ agent: 'codex' })).cmd).toContain('--ask-for-approval');
    expect((await a.setAgentAutoApprove('codex', false)).autoApprove.codex).toBe(false);
    expect((await a.createTerm({ agent: 'codex' })).cmd).not.toContain('--ask-for-approval');
    await expect(m.request('PUT', '/v1/agents/settings', { agent: 'amp', autoApprove: false })).rejects.toMatchObject({ status: 400 });
    await expect(m.request('PUT', '/v1/agents/settings', { agent: 'claude', autoApprove: 'false' })).rejects.toMatchObject({ status: 400 });
  });

  it('starts terms for an agent and validates the request', async () => {
    const { a, events } = setup();
    const term = await a.createTerm({ agent: 'claude', model: 'sonnet', effort: 'max', cwd: '/w' });
    expect(term).toMatchObject({ agent: 'claude', cmd: ['claude', '--permission-mode', 'auto', '--model', 'sonnet', '--effort', 'max'], cwd: '/w', cols: 80, rows: 24 });
    expect(await a.createTerm({ agent: 'agy', effort: 'xhigh', cwd: '/w' })).toMatchObject({ cmd: ['agy', '--mode=auto', '--effort', 'xhigh'] });
    expect((await a.createTerm({ agent: 'shell' })).agent).toBe('shell');
    expect((await a.createTerm({ cmd: ['htop'] })).agent).toBe('shell');
    await expect(a.createTerm({ agent: 'claude', cmd: ['x'] })).rejects.toMatchObject({ status: 400 });
    await expect(a.createTerm({ agent: 'nope' })).rejects.toMatchObject({ status: 400 });
    await expect(a.createTerm({ agent: 'amp' })).rejects.toThrow('not installed');
    expect(events.filter((e) => e.type === 'term.created')).toHaveLength(4);
  });

  it('kills a running term once, with confirmation', async () => {
    const { m, a, events } = setup();
    await expect(m.request('POST', '/v1/terms/tm_shell1/kill', {})).rejects.toMatchObject({ status: 400 });
    expect((await a.killTerm('tm_shell1')) as { exited: boolean }).toMatchObject({ exited: true });
    expect(events.at(-1)!.type).toBe('term.exited');
    await expect(a.killTerm('tm_shell1')).rejects.toMatchObject({ status: 409 });
  });

  it('answers an empty activity without a transcript for agents without session files', async () => {
    const { a } = setup();
    const lines = await a.lines();
    const copilot = lines.find((l) => l.line.agent === 'copilot')!;
    expect(await a.activity(copilot.line.id)).toMatchObject({ lineId: copilot.line.id, transcript: false, workLog: [], todos: [], editedFiles: [] });
    expect((await a.terms()).some((t) => t.lineId === copilot.line.id && t.agent === 'copilot')).toBe(true);
    expect(await a.activity('ln_06')).toMatchObject({ lineId: 'ln_06', transcript: true });
  });

  it('creates projects at the repository root and rejects non repositories', async () => {
    const { a } = setup();
    expect(await a.createProject('~/Documents/Github/work/webshop/src/')).toMatchObject({ path: '/home/alice/Documents/Github/work/webshop', name: 'webshop' });
    const p = await a.createProject('/home/alice/src/billing');
    expect(p).toMatchObject({ path: '/home/alice/src/billing', name: 'billing', repo: 'unknown' });
    expect((await a.projects()).map((x) => x.path)).toContain(p.path);
    await expect(a.createProject('relative/path')).rejects.toMatchObject({ status: 400 });
    await expect(a.createProject('/home/alice/not-a-repo')).rejects.toThrow('not a git repository');
    expect((await new MockDaemon({ live: false }).request<unknown[]>('GET', '/v1/projects')).length).toBe(5);
  });

  it('ranks tasks within their column and patches a rank', async () => {
    const { a } = setup();
    const tasks = await a.tasks();
    const pending = tasks.filter((t) => t.status === 'pending' && t.origin !== 'agent').map((t) => t.rank);
    expect(pending).toEqual([1, 2, 3]);
    const t = await a.createTask({ title: 'Later', description: '', project: '/p', agent: 'gemini', model: '' });
    expect(t).toMatchObject({ rank: 8, effort: 'high', agent: 'gemini' });
    expect((await a.patchTask(t.id, { rank: 0.5 })).rank).toBe(0.5);
  });

  it('discovers new repositories once and creates their auto workspaces', async () => {
    const { a, events } = setup();
    const first = await a.discoverProjects();
    expect(first.added).toBe(1);
    expect(first.projects.map((p) => p.name)).toContain('dotfiles');
    expect(events.filter((e) => e.type === 'workspace.created')).toHaveLength(1);
    expect((await a.workspaces()).find((w) => w.name === 'dotfiles')).toMatchObject({ auto: true });
    expect((await a.discoverProjects()).added).toBe(0);
  });

  it('rescans agents and reports the daemon PATH source', async () => {
    const { a } = setup();
    expect((await a.rescanAgents()).map((x) => x.id)).toEqual((await a.agents()).map((x) => x.id));
    expect((await a.env()).source).toBe('login-shell');
  });

  it('assigns lines and terms to a workspace and clears them when it is deleted', async () => {
    // Given a line, a plain shell term and a user workspace
    // When both are assigned to it, an unknown workspace is tried, then the workspace is deleted
    // Then each assignment is stored with an update event, refused for the unknown one, and cleared
    const { a, events } = setup();
    const w = (await a.workspaces()).find((x) => !x.auto)!;
    const l = (await a.lines())[0].line;
    const sh = (await a.terms()).find((t) => !t.lineId)!;
    expect(await a.assignLineWorkspace(l.id, w.id)).toEqual({ lineId: l.id, workspaceId: w.id });
    expect((await a.assignTermWorkspace(sh.id, w.id)).workspaceId).toBe(w.id);
    expect((await a.lines()).find((x) => x.line.id === l.id)?.local?.workspaceId).toBe(w.id);
    expect(events.filter((e) => e.type === 'line.updated' || e.type === 'term.updated')).toHaveLength(2);
    await expect(a.assignLineWorkspace(l.id, 'nope')).rejects.toMatchObject({ status: 404 });
    await expect(a.assignTermWorkspace('nope', w.id)).rejects.toMatchObject({ status: 404 });
    await a.deleteWorkspace(w.id);
    expect((await a.lines()).find((x) => x.line.id === l.id)?.local?.workspaceId ?? null).toBeNull();
    expect((await a.terms()).find((t) => t.id === sh.id)?.workspaceId ?? null).toBeNull();
  });

  it('creates subjects like the daemon: no path, a shared cwd, strict validation, terms started in it', async () => {
    // Given the mock daemon
    // When two subjects share a cwd, bad bodies are sent and a shell starts for a subject without a cwd
    // Then both exist with a null path, bad input is a 400, the term takes the cwd and the workspace, and cwd patches only apply to subjects
    const { m, a } = setup();
    const post = (b: object) => m.request<any>('POST', '/v1/workspaces', b);
    const s1 = await post({ kind: 'subject', name: 'Deal A', cwd: '~/shared' });
    const s2 = await post({ kind: 'subject', name: 'Deal B', cwd: s1.cwd });
    expect(s1).toMatchObject({ kind: 'subject', path: null });
    expect(s1.cwd).toMatch(/\/shared$/);
    expect(s2.cwd).toBe(s1.cwd);
    for (const b of [{ kind: 'subject', cwd: '/x' }, { kind: 'subject', name: 'x' }, { kind: 'subject', name: 'x', cwd: 'rel' }, { kind: 'subject', name: 'x', cwd: '/x', path: '/x' }, { kind: 'folder', path: '/x' }, { path: '/x', cwd: '/x' }]) {
      await expect(post(b)).rejects.toMatchObject({ status: 400 });
    }
    const t = await m.request<any>('POST', '/v1/terms', { workspaceId: s1.id, cmd: ['zsh'] });
    expect(t).toMatchObject({ cwd: s1.cwd, workspaceId: s1.id });
    await expect(m.request('POST', '/v1/terms', { workspaceId: 'nope' })).rejects.toMatchObject({ status: 404 });
    await expect(m.request('POST', '/v1/terms', { workspaceId: 3 })).rejects.toMatchObject({ status: 400 });
    const p = await m.request<any>('PATCH', `/v1/workspaces/${s1.id}`, { cwd: '/tmp' });
    expect(p.cwd).toBe('/tmp');
    const dir = (await a.workspaces()).find((x) => !x.auto)!;
    await expect(m.request('PATCH', `/v1/workspaces/${dir.id}`, { cwd: '/tmp' })).rejects.toMatchObject({ status: 400 });
    await a.deleteWorkspace(s1.id);
    expect((await a.terms()).find((x) => x.id === t.id)?.workspaceId ?? null).toBeNull();
  });

  it('nests workspaces like the daemon: parent checks, start dir, adoption, the same error messages and cascading delete', async () => {
    // Given the mock daemon and a top-level workspace
    // When nested workspaces are created, a top-level one is adopted, bad bodies are sent and the parent is deleted
    // Then each answer and error message matches the daemon's
    const { m } = setup();
    const post = (b: object) => m.request<any>('POST', '/v1/workspaces', b);
    const err = (p: Promise<unknown>) => p.then(() => '', (e) => `${(e as { status: number }).status} ${(e as Error).message}`);
    const parent = await post({ path: '/home/u/endurance' });
    const n = await post({ path: '/home/u/endurance/20/onboarding', name: 'Onboarding PME', parentId: parent.id, cwd: '/home/u/endurance' });
    expect(n).toMatchObject({ parentId: parent.id, cwd: '/home/u/endurance', name: 'Onboarding PME', folderId: null, pinned: false });
    const subject = await post({ kind: 'subject', name: 'Archi V3', parentId: parent.id });
    expect(subject).toMatchObject({ parentId: parent.id, cwd: '/home/u/endurance', path: null });
    const top = await post({ path: '/home/u/endurance/40/os', pinned: true });
    const adopted = await post({ path: '/home/u/endurance/40/os', parentId: parent.id });
    expect(adopted).toMatchObject({ id: top.id, parentId: parent.id, pinned: false, folderId: null });
    expect(await err(post({ path: '/home/u/elsewhere', parentId: parent.id }))).toBe('400 /home/u/elsewhere is not inside /home/u/endurance');
    expect(await err(post({ path: '/home/u/endurance', parentId: parent.id }))).toBe('400 /home/u/endurance is not inside /home/u/endurance');
    expect(await err(post({ path: '/home/u/endurance/20/onboarding/kyc', parentId: n.id }))).toBe('400 the parent must be a top-level folder workspace');
    expect(await err(post({ path: '/home/u/endurance/x', parentId: 'nope' }))).toBe('400 no workspace nope');
    expect(await err(post({ path: '/home/u/endurance/x', parentId: parent.id, pinned: true }))).toBe('400 a nested workspace has no folder and cannot be pinned');
    expect(await err(post({ path: '/home/u/other', cwd: '/home/u' }))).toBe('400 cwd is only for subjects and nested workspaces');
    expect(await err(m.request('PATCH', `/v1/workspaces/${n.id}`, { folderId: 'f_acme' }))).toBe('400 a nested workspace has no folder and cannot be pinned');
    expect(await err(m.request('PATCH', `/v1/workspaces/${n.id}`, { pinned: true }))).toBe('400 a nested workspace has no folder and cannot be pinned');
    expect(await err(m.request('PATCH', `/v1/workspaces/${parent.id}`, { cwd: '/home/u' }))).toBe('400 cwd is only for subjects and nested workspaces');
    expect((await m.request<any>('PATCH', `/v1/workspaces/${n.id}`, { cwd: null })).cwd).toBeNull();
    const t = await m.request<any>('POST', '/v1/terms', { workspaceId: subject.id, cmd: ['zsh'] });
    expect(t.cwd).toBe('/home/u/endurance');
    const gone = await m.request<any>('DELETE', `/v1/workspaces/${parent.id}`);
    expect(gone.deleted).toBe(parent.id);
    expect(new Set(gone.children)).toEqual(new Set([n.id, subject.id, top.id]));
    const left = await m.request<any[]>('GET', '/v1/workspaces');
    expect(left.some((w) => [parent.id, n.id, subject.id, top.id].includes(w.id))).toBe(false);
    expect((await m.request<any[]>('GET', '/v1/terms')).find((x) => x.id === t.id)?.workspaceId ?? null).toBeNull();
  });

  it('clears the term of a line when the line is unassigned and lets a line inherit its term assignment', async () => {
    // Given a subject and a term started for it on an existing line
    // When the line is unassigned
    // Then the line inherited the assignment at start, and unassigning clears the term too
    const { m, a } = setup();
    const s = await m.request<any>('POST', '/v1/workspaces', { kind: 'subject', name: 'S', cwd: '/tmp' });
    const line = (await a.lines()).find((x) => !x.local?.workspaceId)!.line;
    const t = await m.request<any>('POST', '/v1/terms', { workspaceId: s.id, cmd: ['zsh'], lineId: line.id });
    expect((await a.terms()).find((x) => x.id === t.id)?.workspaceId).toBe(s.id);
    expect((await a.lines()).find((x) => x.line.id === line.id)?.local?.workspaceId).toBe(s.id);
    await a.assignLineWorkspace(line.id, null);
    expect((await a.lines()).find((x) => x.line.id === line.id)?.local?.workspaceId ?? null).toBeNull();
    expect((await a.terms()).find((x) => x.id === t.id)?.workspaceId ?? null).toBeNull();
  });

  it('keeps a missing user workspace in the list', async () => {
    const { a } = setup();
    expect((await a.workspaces()).find((w) => w.missing)).toMatchObject({ auto: false, name: 'thesis' });
  });

  it('fakes uploads with progress and saves pastes to a temp path', async () => {
    const { m } = setup();
    const seen: number[] = [];
    const res = await m.uploadFile('/Users/me/Desktop/Screen Shot.png', (p) => seen.push(p.sent / p.total));
    expect(res).toMatchObject({ name: 'Screen_Shot.png' });
    expect(res.path).toMatch(/^\/home\/alice\/\.overseer\/u\/[a-z0-9]{6}\.png$/);
    expect(res.short).toBe(res.path.replace('/home/alice', '~'));
    expect(seen).toEqual([0, 0.25, 0.5, 0.75, 1]);
    expect(await m.savePaste('image.png', new Uint8Array(3))).toMatch(/^\/tmp\/overseer-paste\/\d+-image\.png$/);
    expect(await m.request('POST', '/v1/uploads?name=a%20b.png')).toMatchObject({ name: 'a_b.png' });
  });

  it('downloads a file through a browser anchor and returns its name', async () => {
    const { m } = setup();
    const a = { href: '', download: '', click: vi.fn() };
    vi.stubGlobal('document', { createElement: vi.fn(() => a) });
    try {
      expect(await m.saveDownload('icon-512.png', new Uint8Array([1, 2, 3]))).toBe('icon-512.png');
      expect(a).toMatchObject({ href: expect.stringMatching(/^blob:/), download: 'icon-512.png' });
      expect(a.click).toHaveBeenCalledOnce();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('stores the auto-organize toggle', async () => {
    const { a } = setup();
    expect(await a.autoOrganize()).toEqual({ enabled: false });
    expect(await a.setAutoOrganize(true)).toEqual({ enabled: true });
    expect(await a.autoOrganize()).toEqual({ enabled: true });
  });

  it('walks the remote connection phases', async () => {
    const { m } = setup();
    const phases: ConnPhase[] = [];
    await m.connect({ kind: 'ssh', host: 'devbox', label: 'devbox' }, (s) => phases.push(s.phase));
    expect(phases).toEqual(['connecting', 'checking', 'installing', 'starting', 'connected']);
    await expect(m.connect({ kind: 'ssh', host: 'unreachable', label: 'x' }, (s) => phases.push(s.phase))).rejects.toThrow();
    expect(phases.at(-1)).toBe('error');
  });

  it('renames and clears a line and a plain terminal with events', async () => {
    // Given the mock daemon with a line and a loose terminal
    // When each is renamed, then the line title is cleared
    // Then the custom titles are cleaned, served, announced, and cleared to null
    const { a, events } = setup();
    const id = (await a.lines())[0].line.id;
    const term = (await a.terms()).find((t) => !t.lineId)!;
    expect(await a.renameLine(id, '  Payments  ')).toEqual({ lineId: id, titleOverride: 'Payments' });
    expect((await a.lines()).find((l) => l.line.id === id)?.local?.titleOverride).toBe('Payments');
    expect(events.at(-1)).toMatchObject({ type: 'line.updated', data: { local: { titleOverride: 'Payments' } } });
    expect((await a.renameTerm(term.id, 'build watch')).titleOverride).toBe('build watch');
    expect(events.at(-1)).toMatchObject({ type: 'term.updated', data: { term: { id: term.id, titleOverride: 'build watch' } } });
    expect(await a.renameLine(id, null)).toEqual({ lineId: id, titleOverride: null });
    await expect(a.renameLine('nope', 'x')).rejects.toMatchObject({ status: 404 });
  });
});

describe('fault injection', () => {
  it('fails matching requests at the configured rate', async () => {
    const f = { rate: 0.5, on: 'POST', hang: '', hangMs: 0 };
    await expect(inject('POST /api/tasks', f, () => 0.2)).rejects.toMatchObject({ status: 503 });
    await expect(inject('POST /api/tasks', f, () => 0.7)).resolves.toBeUndefined();
    await expect(inject('GET /api/state', f, () => 0)).resolves.toBeUndefined();
  });

  it('times out hung requests like the transport does', async () => {
    await expect(inject('GET /api/review', { rate: 0, on: '', hang: '/review', hangMs: 1 })).rejects.toThrow('timed out');
  });

  describe('quorum', () => {
    const mk = (a: ReturnType<typeof setup>['a'], quorum: unknown, extra: Record<string, unknown> = {}) =>
      a.createTask({ title: 'Q', description: '', project: '/p', agent: 'claude', model: '', quorum, ...extra } as never);
    const ok = { workers: ['claude', 'codex'], reviewers: ['codex', 'gemini'] };

    it('stores and returns a valid quorum', async () => {
      const { a } = setup();
      const t = await mk(a, ok);
      expect(t.quorum).toEqual(ok);
      expect((await a.tasks()).find((x) => x.id === t.id)?.quorum).toEqual(ok);
      expect((await mk(a, null)).quorum).toBeUndefined();
    });

    it.each([
      ['a string', 'x', 'quorum must be an object or null'],
      ['an array', [], 'quorum must be an object or null'],
      ['no workers', { workers: [], reviewers: ['codex'] }, 'quorum.workers must be a non-empty array of agent ids'],
      ['non-string workers', { workers: [1], reviewers: ['codex'] }, 'quorum.workers must be a non-empty array of agent ids'],
      ['missing reviewers', { workers: ['claude'] }, 'quorum.reviewers must list 1 or 2 agent ids'],
      ['no reviewer', { workers: ['claude'], reviewers: [] }, 'quorum.reviewers must list 1 or 2 agent ids'],
      ['three reviewers', { workers: ['claude'], reviewers: ['codex', 'gemini', 'agy'] }, 'quorum.reviewers must list 1 or 2 agent ids'],
      ['an unknown agent', { workers: ['nope'], reviewers: ['codex'] }, 'unknown agent nope'],
      ['shell worker', { workers: ['shell'], reviewers: ['codex'] }, 'shell cannot join a quorum'],
      ['shell reviewer', { workers: ['claude'], reviewers: ['shell'] }, 'shell cannot join a quorum'],
      ['duplicate worker', { workers: ['claude', 'claude'], reviewers: ['codex'] }, 'quorum.workers lists claude twice'],
      ['duplicate reviewer', { workers: ['claude'], reviewers: ['codex', 'codex'] }, 'quorum.reviewers lists codex twice'],
    ])('rejects %s', async (_n, quorum, msg) => {
      const { a } = setup();
      await expect(mk(a, quorum)).rejects.toMatchObject({ status: 400, message: msg });
    });

    it('rejects an orchestrator that is not claude or codex', async () => {
      const { a } = setup();
      await expect(mk(a, ok, { agent: 'gemini' })).rejects.toMatchObject({ status: 400, message: 'the quorum orchestrator must be claude or codex' });
    });

    it('rejects a quorum on a subtask', async () => {
      const { a } = setup();
      await expect(mk(a, ok, { parentId: 'tk_01' })).rejects.toMatchObject({ status: 400, message: 'a subtask cannot run a quorum' });
    });

    it('patches and clears the quorum on a pending task only', async () => {
      const { a } = setup();
      const t = await mk(a, null);
      expect((await a.patchTask(t.id, { quorum: ok })).quorum).toEqual(ok);
      expect((await a.patchTask(t.id, { quorum: null })).quorum).toBeUndefined();
      await expect(a.patchTask(t.id, { quorum: 3 as never })).rejects.toMatchObject({ status: 400 });
      await expect(a.patchTask(t.id, { quorum: ok, agent: 'gemini' })).rejects.toMatchObject({ status: 400, message: 'the quorum orchestrator must be claude or codex' });
      await a.startTask(t.id);
      await expect(a.patchTask(t.id, { quorum: ok })).rejects.toMatchObject({ status: 409, message: 'the quorum can only change before the task starts' });
    });

    it('seeds a quorum root, a voted worker and reviewer tasks', async () => {
      const { a } = setup();
      const tasks = await a.tasks();
      expect(tasks.find((t) => t.quorum)).toBeTruthy();
      expect(tasks.find((t) => t.votes)?.votes?.outcome).toBe('rejected');
      expect(tasks.filter((t) => t.origin === 'reviewer').every((t) => t.parentId === 'tk_q2')).toBe(true);
    });
  });
});
