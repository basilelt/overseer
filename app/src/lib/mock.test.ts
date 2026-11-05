import { describe, expect, it } from 'vitest';
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
    expect(new Set(tasks.map((t) => t.status)).size).toBe(5);
    expect(terms.some((t) => !t.lineId)).toBe(true);
    expect(projects.length).toBeGreaterThan(2);
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
    await a.hold('ln_06');
    expect((await a.lines()).find((l) => l.line.id === 'ln_06')!.line.state).toBe('held');
    const term = await a.resume('ln_11');
    expect(term.lineId).toBe('ln_11');
    expect(events.map((e) => e.type)).toContain('line.state_changed');
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
    expect(agents.filter((x) => x.sessions).map((x) => x.id)).toEqual(['claude', 'codex', 'gemini']);
  });

  it('starts terms for an agent and validates the request', async () => {
    const { a, events } = setup();
    const term = await a.createTerm({ agent: 'claude', model: 'sonnet', effort: 'max', cwd: '/w' });
    expect(term).toMatchObject({ agent: 'claude', cmd: ['claude', '--model', 'sonnet', '--effort', 'max'], cwd: '/w', cols: 80, rows: 24 });
    expect((await a.createTerm({ agent: 'shell' })).agent).toBe('shell');
    expect((await a.createTerm({ cmd: ['htop'] })).agent).toBe('shell');
    await expect(a.createTerm({ agent: 'claude', cmd: ['x'] })).rejects.toMatchObject({ status: 400 });
    await expect(a.createTerm({ agent: 'nope' })).rejects.toMatchObject({ status: 400 });
    await expect(a.createTerm({ agent: 'amp' })).rejects.toThrow('not installed');
    expect(events.filter((e) => e.type === 'term.created')).toHaveLength(3);
  });

  it('kills a running term once, with confirmation', async () => {
    const { m, a, events } = setup();
    await expect(m.request('POST', '/v1/terms/tm_shell1/kill', {})).rejects.toMatchObject({ status: 400 });
    expect((await a.killTerm('tm_shell1')) as { exited: boolean }).toMatchObject({ exited: true });
    expect(events.at(-1)!.type).toBe('term.exited');
    await expect(a.killTerm('tm_shell1')).rejects.toMatchObject({ status: 409 });
  });

  it('answers 409 for the activity of agents without session files', async () => {
    const { a } = setup();
    const lines = await a.lines();
    const copilot = lines.find((l) => l.line.agent === 'copilot')!;
    await expect(a.activity(copilot.line.id)).rejects.toMatchObject({ status: 409 });
    expect((await a.terms()).some((t) => t.lineId === copilot.line.id && t.agent === 'copilot')).toBe(true);
    expect((await a.activity('ln_06')).lineId).toBe('ln_06');
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
});
