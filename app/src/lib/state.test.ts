import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FALLBACK_AGENTS } from './agents';
import { DaemonError, setClient } from './client';
import { t } from './i18n.svelte';
import { MockDaemon } from './mock';
import { center } from './notifications.svelte';
import { setPresence } from './presence';
import { pages } from './session-pages.svelte';
import { ui } from './mobile/ui.svelte';
import { settings, workspaceFor } from './settings.svelte';
import { app, NO_WORKSPACE, projectName, statusLine } from './state.svelte';
import { allTabs, closeTabAt, EMPTY_LAYOUT, openInFocused, workspace } from './workspace.svelte';
import type { Activity, ConnState, DaemonEvent, HostTarget, PlatformInfo, Project, SavedHost, Term, Workspace } from './types';

type Handler = (body: unknown) => unknown;

class Daemon extends MockDaemon {
  calls: string[] = [];
  bodies: Record<string, unknown[]> = {};
  override: Record<string, Handler> = {};
  onState: ((s: ConnState) => void) | null = null;
  connectWith: ((target: HostTarget, onState: (s: ConnState) => void) => Promise<void>) | null = null;
  platformInfo: PlatformInfo = { os: 'linux', mobile: false };
  hostList: SavedHost[] = [];
  network: Awaited<ReturnType<MockDaemon['localNetworkStatus']>> = { status: 'notApplicable', host: null, addresses: [], private: false };
  wakes = 0;
  restarts = 0;
  copied: string[] = [];

  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    this.onState = onState;
    if (this.connectWith) return this.connectWith(target, onState);
    onState({ phase: 'connected', target });
  }

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const key = `${method} ${path.split('?')[0]}`;
    this.calls.push(key);
    (this.bodies[key] ??= []).push(body);
    const fn = this.override[key];
    if (fn) return fn(body) as T;
    return super.request<T>(method, path, body);
  }

  override async platform(): Promise<PlatformInfo> {
    return this.platformInfo;
  }

  override async savedHosts(): Promise<SavedHost[]> {
    return this.hostList;
  }

  override async localNetworkStatus(host: string | null = null) {
    return { ...this.network, host };
  }

  override async wake(): Promise<void> {
    this.wakes++;
  }

  override async restartServer(): Promise<void> {
    this.restarts++;
  }

  override async copyText(text: string): Promise<void> {
    this.copied.push(text);
  }

  hits(re: RegExp) {
    return this.calls.filter((c) => re.test(c));
  }
}

const local: HostTarget = { kind: 'local', label: 'Local' };
const ssh: HostTarget = { kind: 'ssh', host: 'box', label: 'box' };
const event = (type: string, data: unknown): DaemonEvent => ({ id: 'e', type: type as DaemonEvent['type'], at: '2026-10-30T12:00:00Z', data });
const fail = (status: number, message = 'boom'): Handler => () => {
  throw new DaemonError(status, message);
};

const snapshot = JSON.parse(JSON.stringify(settings)) as Record<string, unknown>;
const restoreSettings = () => {
  const s = settings as unknown as Record<string, unknown>;
  for (const k of Object.keys(s)) if (!(k in snapshot)) delete s[k];
  Object.assign(s, structuredClone(snapshot));
};

let m: Daemon;

const fresh = () => {
  m = new Daemon({ live: false, delayMs: 0 });
  setClient(m);
  app.client = m;
  return m;
};

const connect = async (target: HostTarget = local) => {
  await app.connect(target);
  m.calls = [];
  m.bodies = {};
};

beforeEach(() => {
  fresh();
});

afterEach(async () => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await app.disconnect();
  app.mobile = false;
  app.view = 'agents';
  app.overlay = null;
  app.toast = null;
  app.menu = null;
  app.browserUrl = '';
  app.sidebarMode = 'workspaces';
  app.nextProfile = null;
  workspace.layout = EMPTY_LAYOUT;
  center.clear();
  restoreSettings();
});

describe('statusLine', () => {
  const act = (over: Partial<Activity>): Activity => ({ lastAssistantText: '', workLog: [], needsInput: false, ...over }) as Activity;

  it('shows the latest work step while the agent works', () => {
    expect(statusLine(act({ workLog: [{ label: 'Reading a.ts' }, { label: 'Editing b.ts' }] as Activity['workLog'], lastAssistantText: 'hello' }))).toBe('Editing b.ts');
  });

  it('shows the first line of the last answer when the agent waits or has no steps', () => {
    expect(statusLine(act({ needsInput: true, workLog: [{ label: 'x' }] as Activity['workLog'], lastAssistantText: 'Which file?\nmore' }))).toBe('Which file?');
    expect(statusLine(act({ lastAssistantText: 'Done.\nDetails' }))).toBe('Done.');
    expect(statusLine(act({ lastAssistantText: 'See `docs/a.md` and ``x``' }))).toBe('See docs/a.md and x');
  });

  it('is empty when there is nothing to say', () => {
    expect(statusLine(act({}))).toBeUndefined();
    expect(statusLine(act({ lastAssistantText: '\nsecond' }))).toBeUndefined();
  });
});

describe('projectName', () => {
  const projects = [{ path: '/src/app', name: 'App' }] as Project[];

  it('prefers the project name, then the last path segment', () => {
    expect(projectName(projects, '/src/app')).toBe('App');
    expect(projectName(projects, '/src/other/')).toBe('other');
    expect(projectName([], '/')).toBe('/');
  });
});

describe('connecting', () => {
  it('opens the connect dialog on a phone with no saved host', async () => {
    m.platformInfo = { os: 'android', mobile: true };
    await app.init();
    expect(app.mobile).toBe(true);
    expect(app.os).toBe('android');
    expect(app.conn).toEqual({ phase: 'idle', target: { kind: 'direct', label: '' } });
    expect(app.overlay).toBe('connect');
    expect(app.connected).toBe(false);
  });

  it('starts a phone demo connected to a sample host, with no prompt', async () => {
    setClient(new MockDaemon({ live: false, delayMs: 0, platform: { os: 'android', mobile: true } }));
    await app.init();
    expect(app.mobile).toBe(true);
    expect(app.conn).toMatchObject({ phase: 'connected', target: { kind: 'direct', host: 'h1' } });
    expect(app.conn.prompt).toBeUndefined();
    expect(app.overlay).toBeNull();
    expect(app.items.length).toBeGreaterThan(0);
  });

  it('connects the desktop to the local daemon and loads everything', async () => {
    await app.init();
    expect(app.connected).toBe(true);
    expect(app.conn.phase).toBe('connected');
    expect(app.items.length).toBeGreaterThan(0);
    expect(app.profiles).not.toBeNull();
    expect(app.overlay).toBeNull();
    expect(settings.explicitlyDisconnected).toBe(false);
  });

  it('opens the connect dialog when the remembered SSH host fails', async () => {
    settings.lastHost = ssh;
    m.connectWith = async (target, onState) => {
      onState({ phase: 'error', target, error: 'ssh: connect to host box port 22: No route to host' });
      throw new Error('exit 255');
    };
    m.network = { status: 'denied', host: null, addresses: [], private: true };
    await app.init();
    expect(app.conn).toMatchObject({ phase: 'error', target: ssh, error: 'ssh: connect to host box port 22: No route to host' });
    expect(app.overlay).toBe('connect');
    expect(app.localNet).toMatchObject({ status: 'denied', host: 'box' });
    expect(app.connected).toBe(false);
  });

  it('keeps the thrown message when the client did not report an error state', async () => {
    m.connectWith = async () => {
      throw new Error('spawn failed');
    };
    await app.connect(ssh);
    expect(app.conn).toMatchObject({ phase: 'error', error: 'spawn failed' });
    expect(app.localNet).toBeNull();
  });

  it('remembers that local network access was asked once it applies', async () => {
    m.network = { status: 'granted', host: null, addresses: [], private: true };
    await app.connect(ssh);
    expect(settings.localNetworkAsked).toBe(true);
  });

  it('does not remember the question when it does not apply', async () => {
    await app.connect(ssh);
    expect(settings.localNetworkAsked).toBe(false);
  });

  it('probes nothing when the local network status fails', async () => {
    m.localNetworkStatus = async () => {
      throw new Error('unsupported');
    };
    await app.askLocalNetwork('box');
    expect(await app.probeLocalNetwork('box')).toBeNull();
    expect(settings.localNetworkAsked).toBe(false);
  });

  it('reloads the data when a dropped connection comes back', async () => {
    await connect();
    m.onState!({ phase: 'reconnecting', target: local, attempt: 1 });
    expect(m.hits(/GET \/v1\/lines$/)).toHaveLength(0);
    m.onState!({ phase: 'connected', target: local });
    await vi.waitFor(() => expect(m.hits(/GET \/v1\/lines$/)).toHaveLength(1));
    m.onState!({ phase: 'connected', target: local });
    await new Promise((r) => setTimeout(r, 0));
    expect(m.hits(/GET \/v1\/lines$/)).toHaveLength(1);
  });

  it('selects nothing from another workspace when a dropped connection comes back on an empty workspace', async () => {
    await connect();
    const empty = await app.api.createWorkspace({ path: '/srv/newproj' });
    await app.refresh();
    app.selectWorkspace(empty.id);
    app.selectedKey = null;
    expect(app.membersOf(empty.id)).toEqual([]);
    m.onState!({ phase: 'reconnecting', target: local, attempt: 1 });
    m.onState!({ phase: 'connected', target: local });
    await vi.waitFor(() => expect(m.hits(/GET \/v1\/lines$/)).toHaveLength(2));
    await vi.waitFor(() => expect((app as unknown as { refreshing: number }).refreshing).toBe(0));
    expect(app.workspaceId).toBe(empty.id);
    expect(app.selectedKey).toBeNull();
  });

  it('selects the first item of the current workspace when it has no saved tabs', async () => {
    await connect();
    const ws = app.ws.workspaces.find((w) => app.membersOf(w.id).length && w.id !== app.workspaceId) ?? app.currentWorkspace!;
    app.selectWorkspace(ws.id);
    app.selectedKey = null;
    await app.refresh();
    expect(app.selectedKey).not.toBeNull();
    expect(app.membersOf(ws.id).map((i) => i.key)).toContain(app.selectedKey);
  });

  it('selects the first member of the current group when nothing is selected', async () => {
    await connect();
    const last = app.items[app.items.length - 1].key;
    const g = await app.api.createGroup({ name: 'solo', members: [last] });
    await app.refresh();
    app.selectWorkspace(`group:${g.id}`);
    app.selectedKey = null;
    await app.refresh();
    expect(app.selectedKey).toBe(last);
  });

  it('selects the first item of every workspace in the status view', async () => {
    await connect();
    app.setSidebarMode('status');
    app.selectedKey = null;
    await app.refresh();
    expect(app.selectedKey).toBe(app.groups.find((g) => g.items.length)?.items[0].key);
  });

  it('forgets the session on disconnect and remembers the choice', async () => {
    await connect();
    workspace.layout = openInFocused(EMPTY_LAYOUT, 'line:ln_01');
    app.nextProfile = 'p1';
    await app.disconnect();
    expect(app.connected).toBe(false);
    expect(app.conn).toEqual({ phase: 'idle', target: local });
    expect(app.data.lines).toEqual([]);
    expect(app.items).toEqual([]);
    expect(app.agents).toEqual([]);
    expect(app.profiles).toBeNull();
    expect(app.nextProfile).toBeNull();
    expect(app.selectedKey).toBeNull();
    expect(app.workspaceId).toBe(NO_WORKSPACE);
    expect(workspace.layout).toBe(EMPTY_LAYOUT);
    expect(settings.explicitlyDisconnected).toBe(true);
  });

  it('stops listening to daemon events after disconnect', async () => {
    await connect();
    const seen = vi.fn();
    app.onBroadcast(seen);
    await app.disconnect();
    m.emit('broadcast.delivered', { id: 'b', target: 'x', result: 'sent' });
    expect(seen).not.toHaveBeenCalled();
  });

  it('wakes the daemon only on mobile', async () => {
    app.wake();
    app.mobile = true;
    app.wake();
    expect(m.wakes).toBe(1);
  });

  it('forwards a server restart to the client', async () => {
    await app.restartServer();
    expect(m.restarts).toBe(1);
  });
});

describe('refresh feedback watch', () => {
  it('re-attaches to a running feedback terminal on connect', async () => {
    const attached: string[] = [];
    m.attachTerm = async (id) => {
      attached.push(id);
      return { write: () => {}, resize: () => {}, close: () => {} };
    };
    const base = await m.request<Term[]>('GET', '/v1/terms');
    m.override['GET /v1/terms'] = () => [...base, { ...base[0], id: 'fb1', title: 'Feedback', cmd: ['claude', '--x'], exited: false, taskId: undefined, lineId: undefined }];
    await app.connect(local);
    expect(attached).toEqual(['fb1']);
  });
});

describe('refresh', () => {
  it('does nothing while disconnected', async () => {
    await app.refresh();
    expect(m.calls).toEqual([]);
  });

  it('loads activity only for live sessions of agents with session logs', async () => {
    await app.connect(local);
    const fetched = m.hits(/\/activity$/).map((c) => c.split('/')[3]);
    expect(fetched.sort()).toEqual(['ln_01', 'ln_02', 'ln_03', 'ln_04', 'ln_05', 'ln_06', 'ln_07']);
  });

  it('is synced only once a complete snapshot landed, not on events that arrive first', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const all = await m.request<unknown>('GET', '/v1/lines');
    m.override['GET /v1/lines'] = () => gate.then(() => all);
    const pending = app.connect(local);
    await vi.waitFor(() => expect(m.hits(/\/v1\/lines$/).length).toBe(2));
    m.emit('line.updated', { line: { ...(all as { line: object }[])[0].line, id: 'ln_early' } });
    expect(app.items.map((i) => i.key)).toEqual(['line:ln_early']);
    expect(app.connected).toBe(true);
    expect(app.synced).toBe(false);
    release();
    await pending;
    expect(app.synced).toBe(true);
    m.override['GET /v1/lines'] = fail(500);
    await app.connect(local);
    expect(app.synced).toBe(false);
  });

  it('keeps the held state an event delivered while the lines request was still in flight', async () => {
    await connect();
    const before = await m.request<{ line: { id: string; state: string } }[]>('GET', '/v1/lines');
    const target = before.find((l) => l.line.state === 'active')!.line;
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    m.override['GET /v1/lines'] = () => gate.then(() => before);
    const pending = app.refresh();
    await vi.waitFor(() => expect(m.hits(/\/v1\/lines$/).length).toBe(2));
    m.emit('line.state_changed', { line: { ...target, state: 'held' }, from: 'active', to: 'held' });
    expect(app.data.lines.find((l) => l.line.id === target.id)?.line.state).toBe('held');
    release();
    await pending;
    expect(app.data.lines.find((l) => l.line.id === target.id)?.line.state).toBe('held');
    expect(app.data.lines.length).toBe(before.length);
  });

  it('takes the fetched lines once no refresh is in flight, so a later refresh is not held back by old events', async () => {
    await connect();
    const before = await m.request<{ line: { id: string; state: string } }[]>('GET', '/v1/lines');
    const target = before.find((l) => l.line.state === 'active')!.line;
    m.emit('line.state_changed', { line: { ...target, state: 'held' }, from: 'active', to: 'held' });
    await app.refresh();
    expect(app.data.lines.find((l) => l.line.id === target.id)?.line.state).toBe('active');
  });

  it('keeps a task an event deleted while the tasks request was in flight deleted', async () => {
    await connect();
    const before = await m.request<{ id: string }[]>('GET', '/v1/tasks');
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    m.override['GET /v1/tasks'] = () => gate.then(() => before);
    const pending = app.refresh();
    await vi.waitFor(() => expect(m.hits(/\/v1\/tasks$/).length).toBe(2));
    m.emit('task.deleted', { id: before[0].id });
    release();
    await pending;
    expect(app.data.tasks.map((t) => t.id)).not.toContain(before[0].id);
  });

  it('falls back to the built-in agent list on an older daemon', async () => {
    m.override['GET /v1/agents'] = fail(404);
    await app.connect(local);
    expect(app.agents).toBe(FALLBACK_AGENTS);
  });

  it('keeps the known agents when the agent list fails', async () => {
    await connect();
    const known = app.agents;
    m.override['GET /v1/agents'] = fail(500);
    await app.refresh();
    expect(app.agents).toBe(known);
  });

  it('keeps the previous data when parts of it fail to load', async () => {
    await connect();
    const { lines, terms, tasks } = app.data;
    const projects = app.projects;
    for (const k of ['lines', 'terms', 'tasks', 'projects']) m.override[`GET /v1/${k}`] = fail(500);
    await app.refresh();
    expect(app.data.lines).toBe(lines);
    expect(app.data.terms).toBe(terms);
    expect(app.data.tasks).toBe(tasks);
    expect(app.projects).toBe(projects);
  });

  it('refuses API calls without a client', () => {
    const client = app.client;
    app.client = null;
    try {
      expect(() => app.api).toThrow('not connected');
    } finally {
      app.client = client;
    }
  });

  it('builds workspaces from projects when the daemon has no workspaces API', async () => {
    m.override['GET /v1/workspaces'] = fail(404);
    await app.connect(local);
    expect(app.wsPersistent).toBe(false);
    expect(app.ws.folders).toEqual([]);
    expect(app.ws.workspaces.map((w) => w.path)).toEqual(expect.arrayContaining(app.projects.map((p) => p.path)));
  });

  it('selects the first item when nothing is selected', async () => {
    await app.connect(local);
    expect(app.selectedKey).toBe(app.groups.find((g) => g.items.length)?.items[0].key);
  });

  it('uploads projects saved by older versions and keeps their colours', async () => {
    settings.extraProjects = [
      { path: '/srv/legacy', name: 'legacy', color: '#ff0000' } as Project,
      { path: '/srv/broken', name: 'broken' } as Project,
    ];
    m.override['POST /v1/projects'] = (body) => {
      const path = (body as { path: string }).path;
      if (path === '/srv/broken') throw new DaemonError(400, 'bad');
      return { path, name: 'legacy' };
    };
    await app.connect(local);
    await vi.waitFor(() => expect(settings.extraProjects).toEqual([]));
    expect(m.bodies['POST /v1/projects']).toEqual([{ path: '/srv/legacy' }, { path: '/srv/broken' }]);
    expect(app.projects.filter((p) => p.path === '/srv/legacy')).toHaveLength(1);
    expect(settings.projectColors['/srv/legacy']).toBe('#ff0000');
    expect(app.allProjects.find((p) => p.path === '/srv/legacy')?.color).toBe('#ff0000');
  });

  it('opens the project tab requested in the URL once loaded', async () => {
    vi.useFakeTimers();
    (app as unknown as { pendingTab: string | null }).pendingTab = 'graph';
    await app.connect(local);
    expect(allTabs(workspace.layout)).toEqual([]);
    await vi.advanceTimersByTimeAsync(0);
    const key = app.selectedKey!;
    expect(key).toContain(app.currentWorkspace!.path);
    expect(allTabs(workspace.layout)).toEqual([key]);
  });
});

describe('daemon events', () => {
  beforeEach(async () => {
    await connect();
  });

  it('applies workspace events without notifying session listeners', () => {
    const changed = vi.fn();
    app.onChange(changed);
    const w = { ...app.ws.workspaces[0], id: 'ws_new', name: 'New' } as Workspace;
    m.emit('workspace.created', { workspace: w });
    expect(app.ws.workspaces.some((x) => x.id === 'ws_new')).toBe(true);
    expect(changed).not.toHaveBeenCalled();
  });

  it('adds a project announced after the connection', () => {
    const p: Project = { path: '/late/repo', name: 'repo', repo: 'late/repo' };
    m.emit('project.added', { project: p });
    expect(app.projects.some((x) => x.path === '/late/repo')).toBe(true);
    m.emit('project.added', { project: p });
    expect(app.projects.filter((x) => x.path === '/late/repo')).toHaveLength(1);
    m.emit('project.added', {});
  });

  it('rechecks the workspaces when the window comes back to the foreground', async () => {
    setPresence({ focused: false });
    const before = m.calls.filter((c) => c === 'GET /v1/workspaces').length;
    m.override['GET /v1/workspaces'] = () => [{ ...app.ws.workspaces[0], missing: true }];
    setPresence({ focused: true });
    await vi.waitFor(() => expect(app.ws.workspaces[0].missing).toBe(true));
    expect(m.calls.filter((c) => c === 'GET /v1/workspaces').length).toBe(before + 1);
  });

  it('stops rechecking the workspaces once disconnected', async () => {
    await app.disconnect();
    const before = m.calls.length;
    setPresence({ focused: false });
    setPresence({ focused: true });
    expect(m.calls.length).toBe(before);
  });

  it('reloads the profiles when they change', async () => {
    m.emit('profiles.changed', {});
    await vi.waitFor(() => expect(m.calls).toContain('GET /v1/profiles'));
  });

  it('clears the profiles when they fail to load', async () => {
    m.override['GET /v1/profiles'] = fail(500);
    await app.loadProfiles();
    expect(app.profiles).toBeNull();
  });

  it('saves profiles through the daemon', async () => {
    await app.saveProfiles({ default: 'economy' });
    expect(m.bodies['PUT /v1/profiles']).toEqual([{ default: 'economy' }]);
    expect(app.profiles?.default).toBe('economy');
  });

  it('routes broadcast, organize, task message and chat events to their own listeners', () => {
    const broadcast = vi.fn();
    const organize = vi.fn();
    const message = vi.fn();
    const chat = vi.fn();
    const changed = vi.fn();
    const offs = [app.onBroadcast(broadcast), app.onOrganize(organize), app.onTaskMessage(message), app.onChat(chat), app.onChange(changed)];
    m.emit('broadcast.delivered', { id: 'b1', target: 'line:ln_01', result: 'sent' });
    m.emit('organize.proposal', { proposal: { id: 'p1' } });
    m.emit('organize.proposal', {});
    app.onEvent(event('task.message', { message: { id: 'm1' } }));
    app.onEvent(event('task.message', { message: {} }));
    app.onEvent(event('line.chat', { lineId: 'ln_01', cursor: 3 }));
    app.onEvent(event('line.chat', {}));
    expect(broadcast).toHaveBeenCalledExactlyOnceWith({ id: 'b1', target: 'line:ln_01', result: 'sent' });
    expect(organize).toHaveBeenCalledExactlyOnceWith({ id: 'p1' });
    expect(message).toHaveBeenCalledExactlyOnceWith({ id: 'm1' });
    expect(chat).toHaveBeenCalledExactlyOnceWith({ lineId: 'ln_01', cursor: 3 });
    expect(changed).not.toHaveBeenCalled();
    for (const off of offs) off();
    m.emit('broadcast.delivered', { id: 'b2', target: 'x', result: 'sent' });
    app.onEvent(event('line.chat', { lineId: 'ln_01', cursor: 4 }));
    expect(broadcast).toHaveBeenCalledTimes(1);
    expect(chat).toHaveBeenCalledTimes(1);
  });

  it('records a failed PR review in the notification center', async () => {
    app.onEvent(event('review.failed', { repo: 'o/r', number: 5, error: 'no token' }));
    await vi.waitFor(() => expect(center.entries).toHaveLength(1));
    expect(center.entries[0]).toMatchObject({ kind: 'error', source: 'o/r#5' });
    expect(center.entries[0].title).toContain('no token');
  });

  it('names a failed review by its title when it has no PR number', async () => {
    app.onEvent(event('review.failed', { title: 'Local diff' }));
    await vi.waitFor(() => expect(center.entries).toHaveLength(1));
    expect(center.entries[0].source).toBe('Local diff');
  });

  it('toasts why a queued pause was cancelled when the daemon refuses it', () => {
    app.onEvent(event('line.hold_refused', { lineId: 'ln_01', failed: [{ name: 'noToolRunning', detail: 'a tool subprocess is running' }], error: null }));
    expect(app.toast).toMatchObject({ text: t('chat.pauseRefused', { reason: 'a tool subprocess is running' }), kind: 'info' });
    expect(app.toast?.text).toBe('Pause cancelled: a tool subprocess is running');
  });

  it('joins every failed check detail and falls back to the check name or the error', () => {
    app.onEvent(event('line.hold_refused', { lineId: 'ln_01', failed: [{ name: 'noToolRunning', detail: 'a subagent is running' }, { name: 'notPinned' }], error: null }));
    expect(app.toast?.text).toBe('Pause cancelled: a subagent is running, notPinned');
    app.onEvent(event('line.hold_refused', { lineId: 'ln_01', failed: [], error: 'daemon busy' }));
    expect(app.toast?.text).toBe('Pause cancelled: daemon busy');
    app.onEvent(event('line.hold_refused', { lineId: 'ln_01' }));
    expect(app.toast?.text).toBe('Pause cancelled');
  });

  it('notifies change listeners of session updates and unsubscribes', () => {
    const changed = vi.fn();
    const off = app.onChange(changed);
    const term = { ...app.data.terms[0], id: 'tm_new', lineId: undefined };
    m.emit('term.created', { term });
    expect(changed).toHaveBeenCalledTimes(1);
    const [prev, next, e] = changed.mock.calls[0];
    expect(prev.terms.some((x: { id: string }) => x.id === 'tm_new')).toBe(false);
    expect(next.terms.some((x: { id: string }) => x.id === 'tm_new')).toBe(true);
    expect(e.type).toBe('term.created');
    expect(app.data).toBe(next);
    off();
    m.emit('term.created', { term: { ...term, id: 'tm_new2' } });
    expect(changed).toHaveBeenCalledTimes(1);
  });

  it('skips listeners for events that change nothing, except git and review events', () => {
    const changed = vi.fn();
    app.onChange(changed);
    const data = app.data;
    app.onEvent(event('ports.changed', {}));
    expect(changed).not.toHaveBeenCalled();
    app.onEvent(event('git.changed', { repo: '/x' }));
    app.onEvent(event('review.updated', {}));
    app.onEvent(event('worktree.created', {}));
    expect(changed).toHaveBeenCalledTimes(3);
    expect(app.data).toBe(data);
  });

  it('fetches the activity of a background session once after a burst of events', async () => {
    vi.useFakeTimers();
    app.selectedKey = 'line:ln_01';
    app.onEvent(event('line.activity', { lineId: 'ln_03' }));
    app.onEvent(event('line.activity', { lineId: 'ln_03' }));
    await vi.advanceTimersByTimeAsync(2499);
    expect(m.hits(/activity$/)).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(m.hits(/activity$/)).toEqual(['GET /v1/lines/ln_03/activity']);
    app.onEvent(event('line.activity', { lineId: 'ln_03' }));
    await vi.advanceTimersByTimeAsync(2500);
    expect(m.hits(/activity$/)).toHaveLength(2);
  });

  it('fetches the activity of the selected session sooner', async () => {
    vi.useFakeTimers();
    app.selectedKey = 'line:ln_03';
    app.onEvent(event('line.activity', { lineId: 'ln_03' }));
    await vi.advanceTimersByTimeAsync(400);
    expect(m.hits(/activity$/)).toEqual(['GET /v1/lines/ln_03/activity']);
  });

  it('does not fetch activity that came with the event', async () => {
    vi.useFakeTimers();
    app.onEvent(event('line.activity', { lineId: 'ln_03', activity: app.data.activity.ln_03 }));
    await vi.advanceTimersByTimeAsync(5000);
    expect(m.hits(/activity$/)).toEqual([]);
  });
});

describe('loadActivity', () => {
  it('returns nothing while disconnected', async () => {
    expect(await app.loadActivity('ln_01')).toBeNull();
    expect(m.calls).toEqual([]);
  });

  it('skips agents without session logs', async () => {
    await connect();
    expect(await app.loadActivity('ln_10')).toBeNull();
    expect(m.calls).toEqual([]);
  });

  it('returns nothing for a line without a transcript and leaves the snapshot alone', async () => {
    await connect();
    m.override['GET /v1/lines/ln_03/activity'] = () => ({ lineId: 'ln_03', title: '', goal: '', status: 'active', needsInput: false, busy: false, workLog: [], todos: [], editedFiles: [], lastAssistantText: '', transcript: false });
    const data = app.data;
    expect(await app.loadActivity('ln_03')).toBeNull();
    expect(app.data).toBe(data);
  });

  it('returns nothing when the request fails', async () => {
    await connect();
    m.override['GET /v1/lines/ln_03/activity'] = fail(500);
    const data = app.data;
    expect(await app.loadActivity('ln_03')).toBeNull();
    expect(app.data).toBe(data);
  });

  it('keeps the same snapshot when nothing changed', async () => {
    await connect();
    const prev = app.data.activity.ln_03;
    const data = app.data;
    expect(await app.loadActivity('ln_03')).toBe(prev);
    expect(app.data).toBe(data);
  });

  it('updates the waiting and busy flags of the session from its activity', async () => {
    await connect();
    const act = { ...structuredClone(app.data.activity.ln_03), needsInput: true, busy: true };
    m.override['GET /v1/lines/ln_03/activity'] = () => act;
    const lines = app.data.lines;
    expect(await app.loadActivity('ln_03')).toEqual(act);
    const l = app.data.lines.find((x) => x.line.id === 'ln_03')!;
    expect(l.local).toMatchObject({ needsInput: true, busy: true });
    expect(app.data.lines.find((x) => x.line.id === 'ln_01')).toBe(lines.find((x) => x.line.id === 'ln_01'));
    expect(app.data.activity.ln_03).toEqual(act);
  });

  it('keeps the busy flag when the activity does not report it', async () => {
    await connect();
    const act = { ...structuredClone(app.data.activity.ln_04), needsInput: true, busy: undefined };
    m.override['GET /v1/lines/ln_04/activity'] = () => act;
    await app.loadActivity('ln_04');
    expect(app.data.lines.find((x) => x.line.id === 'ln_04')!.local).toMatchObject({ needsInput: true, busy: true });
  });
});

describe('workspaces', () => {
  beforeEach(async () => {
    await connect();
  });

  it('lists the members of a workspace and of a group', () => {
    expect(app.membersOf('ws_proxmox').map((i) => i.key)).toEqual(['line:ln_07', 'line:ln_11']);
    expect(app.groupItems('g_infra').map((i) => i.key).sort()).toEqual(['line:ln_04', 'line:ln_07', 'line:ln_11']);
    expect(app.groupItems('missing')).toEqual([]);
  });

  it('matches group members saved as terminal keys', () => {
    app.applyWs({ ...app.ws, groups: [{ ...app.ws.groups[0], id: 'g_t', members: ['term:tm_ln_03'] }] });
    expect(app.groupItems('g_t').map((i) => i.key)).toEqual(['line:ln_03']);
  });

  it('remembers the chosen workspace per host and leaves the browser view', () => {
    app.view = 'browser';
    app.selectWorkspace('ws_proxmox');
    expect(app.workspaceId).toBe('ws_proxmox');
    expect(app.currentWorkspace?.id).toBe('ws_proxmox');
    expect(app.view).toBe('agents');
    expect(workspaceFor(local)).toBe('ws_proxmox');
  });

  it('stays on the board when switching workspace from it', () => {
    app.view = 'kanban';
    app.selectWorkspace('ws_webshop');
    expect(app.view).toBe('kanban');
  });

  it('reopens the remembered workspace on the next connection', async () => {
    app.selectWorkspace('ws_oslabs');
    await app.disconnect();
    await app.connect(local);
    expect(app.workspaceId).toBe('ws_oslabs');
  });

  it('keeps the sidebar mode across launches', () => {
    app.setSidebarMode('status');
    expect(app.sidebarMode).toBe('status');
    expect(settings.sidebarMode).toBe('status');
  });

  it('adds a new workspace and replaces a known one', () => {
    const w = app.ws.workspaces[0];
    app.upsertWorkspace({ ...w, name: 'Renamed' });
    app.upsertWorkspace({ ...w, id: 'ws_added' });
    expect(app.ws.workspaces.find((x) => x.id === w.id)?.name).toBe('Renamed');
    expect(app.ws.workspaces.filter((x) => x.id === w.id)).toHaveLength(1);
    expect(app.ws.workspaces.at(-1)?.id).toBe('ws_added');
  });

  it('patches a workspace optimistically and keeps the daemon answer', async () => {
    let seen: string | undefined;
    m.override['PATCH /v1/workspaces/ws_webshop'] = (body) => {
      seen = app.ws.workspaces.find((w) => w.id === 'ws_webshop')?.name;
      return { ...app.ws.workspaces.find((w) => w.id === 'ws_webshop')!, ...(body as object), rank: 42 };
    };
    await app.patchWorkspace('ws_webshop', { name: 'Acme' });
    expect(seen).toBe('Acme');
    expect(app.ws.workspaces.find((w) => w.id === 'ws_webshop')).toMatchObject({ name: 'Acme', rank: 42 });
  });

  it('rolls a failed patch back and reports it', async () => {
    const before = app.ws.workspaces.find((w) => w.id === 'ws_webshop')!;
    m.override['PATCH /v1/workspaces/ws_webshop'] = fail(409, 'name taken');
    await app.patchWorkspace('ws_webshop', { name: 'Acme' });
    expect(app.ws.workspaces.find((w) => w.id === 'ws_webshop')).toEqual(before);
    expect(app.toast).toMatchObject({ text: 'name taken', kind: 'error' });
  });

  it('patches only locally when workspaces are not stored by the daemon', async () => {
    app.wsPersistent = false;
    await app.patchWorkspace('ws_webshop', { pinned: true });
    expect(app.ws.workspaces.find((w) => w.id === 'ws_webshop')?.pinned).toBe(true);
    expect(m.calls).toEqual([]);
  });

  it('ignores a patch for an unknown workspace', async () => {
    const ws = app.ws;
    await app.patchWorkspace('ws_gone', { name: 'x' });
    expect(app.ws).toBe(ws);
    expect(m.calls).toEqual([]);
  });

  it('keeps a selected group or the no-workspace view across refreshes', async () => {
    app.selectWorkspace('group:g_infra');
    await app.refresh();
    expect(app.workspaceId).toBe('group:g_infra');
    app.selectWorkspace(NO_WORKSPACE);
    await app.refresh();
    expect(app.workspaceId).toBe(NO_WORKSPACE);
  });

  it('leaves a selected group once it is deleted', async () => {
    app.selectWorkspace('group:g_infra');
    m.groups = m.groups.filter((g) => g.id !== 'g_infra');
    await app.refresh();
    expect(app.workspaceId).not.toBe('group:g_infra');
  });

  it('falls back to the first live workspace when the selected one disappears', async () => {
    app.selectWorkspace('ws_thesis');
    m.workspaces = m.workspaces.filter((w) => w.id !== 'ws_thesis');
    await app.refresh();
    expect(app.workspaceId).not.toBe('ws_thesis');
    expect(app.ws.workspaces.some((w) => w.id === app.workspaceId)).toBe(true);
  });
});

describe('selecting items', () => {
  beforeEach(async () => {
    await connect();
  });

  it('switches to the workspace of the selected session', () => {
    app.selectWorkspace('ws_webshop');
    app.view = 'kanban';
    app.select('line:ln_07');
    expect(app.workspaceId).toBe('ws_proxmox');
    expect(app.selectedKey).toBe('line:ln_07');
    expect(app.view).toBe('agents');
  });

  it('stays in the workspace when the item is already open in a tab', () => {
    app.selectWorkspace('ws_webshop');
    workspace.layout = openInFocused(EMPTY_LAYOUT, 'line:ln_07');
    app.select('line:ln_07');
    expect(app.workspaceId).toBe('ws_webshop');
  });

  it('stays in the workspace for tabs that are not sessions', () => {
    app.selectWorkspace('ws_webshop');
    app.select('graph:/x');
    expect(app.workspaceId).toBe('ws_webshop');
    app.select(null);
    expect(app.selectedKey).toBeNull();
  });

  it('does not switch workspace in the status sidebar', () => {
    app.selectWorkspace('ws_webshop');
    app.sidebarMode = 'status';
    app.select('line:ln_07');
    expect(app.workspaceId).toBe('ws_webshop');
  });

  it('records which overlay is open and with which argument', () => {
    app.open('profiles', 'global');
    expect([app.overlay, app.overlayArg, app.fileArg]).toEqual(['profiles', 'global', null]);
    app.open('file', { path: '/a' });
    expect([app.overlay, app.overlayArg, app.fileArg]).toEqual(['file', null, { path: '/a' }]);
  });

  it('opens a menu at the pointer', () => {
    app.openMenu({ clientX: 10, clientY: 20 }, 'Tab', []);
    expect(app.menu).toEqual({ items: [], x: 10, y: 20, label: 'Tab' });
  });
});

describe('sessions', () => {
  beforeEach(async () => {
    await connect();
  });

  it('starts an agent in a worktree of the current workspace project', async () => {
    app.selectWorkspace('ws_webshop');
    const term = await app.newSession('claude', { model: 'opus' });
    expect(m.bodies['POST /v1/terms']).toEqual([{ agent: 'claude', cwd: app.currentWorkspace!.path, workspaceId: 'ws_webshop', model: 'opus', effort: undefined, cols: 120, rows: 32, worktree: true }]);
    expect(app.data.terms.some((x) => x.id === term.id)).toBe(true);
    expect(app.selectedKey).toBe(`term:${term.id}`);
    expect(app.view).toBe('agents');
  });

  describe('following a session just started', () => {
    const lineFor = async () => {
      const all = await m.request<{ line: { id: string } }[]>('GET', '/v1/lines');
      return { ...all[0].line, id: 'ln_new', state: 'active' };
    };

    it('selects the line when the term gets linked before the line is known', async () => {
      const term = await app.newSession('claude');
      const line = await lineFor();
      m.emit('term.updated', { term: { ...term, lineId: line.id } });
      expect(app.items.some((i) => i.key === `term:${term.id}`)).toBe(false);
      m.emit('line.updated', { line });
      expect(app.items.find((i) => i.lineId === line.id)?.termId).toBe(term.id);
      expect(app.selectedKey).toBe(`line:${line.id}`);
      expect(app.selected?.lineId).toBe(line.id);
    });

    it('selects the line when it arrives together with the term link', async () => {
      const term = await app.newSession('claude');
      const line = await lineFor();
      m.emit('line.updated', { line });
      m.emit('term.updated', { term: { ...term, lineId: line.id } });
      expect(app.selectedKey).toBe(`line:${line.id}`);
    });

    it('leaves the selection alone once the user picked another session', async () => {
      const term = await app.newSession('claude');
      const line = await lineFor();
      m.emit('term.updated', { term: { ...term, lineId: line.id } });
      const other = app.items.find((i) => i.key.startsWith('line:'))!.key;
      app.select(other);
      m.emit('line.updated', { line });
      expect(app.selectedKey).toBe(other);
    });

    it('stops following when the term is gone', async () => {
      const term = await app.newSession('claude');
      m.emit('term.exited', { term: { ...term, exited: true } });
      const line = await lineFor();
      m.emit('line.updated', { line });
      expect(app.selectedKey).toBe(`term:${term.id}`);
    });

    it('opens the workspace that holds a session started from elsewhere', async () => {
      app.selectWorkspace('ws_webshop');
      const base = await app.newSession('shell');
      const term = { ...base, id: 'tm_feedback', cwd: '/tmp/elsewhere', workspaceId: null } as Term;
      app.followSession(term);
      expect(app.selectedKey).toBe('term:tm_feedback');
      expect(app.workspaceId).toBe(NO_WORKSPACE);
    });
  });

  it('lists a session under its workspace before it reports a line', async () => {
    app.selectWorkspace('ws_webshop');
    const term = await app.newSession('codex');
    expect(term.cwd).not.toBe(app.currentWorkspace!.path);
    expect(app.itemWs.get(`term:${term.id}`)).toBe('ws_webshop');
  });

  it('assigns a shell started in the workspace too', async () => {
    app.selectWorkspace('ws_webshop');
    await app.newSession('shell');
    expect((m.bodies['POST /v1/terms'].at(-1) as { workspaceId?: string }).workspaceId).toBe('ws_webshop');
  });

  it('does not assign a session started in a directory outside the workspace', async () => {
    app.selectWorkspace('ws_webshop');
    await app.newSession('claude', { cwd: '/tmp' });
    expect((m.bodies['POST /v1/terms'].at(-1) as { workspaceId?: string }).workspaceId).toBeUndefined();
  });

  it('starts in the checkout when the user prefers it or outside a project', async () => {
    settings.sessionsInCheckout = true;
    await app.newSession('claude', { cwd: '/home/alice/Documents/Github/work/webshop' });
    settings.sessionsInCheckout = false;
    await app.newSession('claude', { cwd: '/tmp' });
    await app.newSession('claude', { cwd: '/home/alice/Documents/Github/work/webshop', worktree: false });
    expect(m.bodies['POST /v1/terms'].map((b) => (b as { worktree?: boolean }).worktree)).toEqual([undefined, undefined, undefined]);
  });

  describe('in a subject', () => {
    const repo = '/home/alice/Documents/Github/work/webshop';
    const subject = async () => {
      const w = await m.request<{ id: string }>('POST', '/v1/workspaces', { kind: 'subject', name: 'Deal', cwd: repo });
      await vi.waitFor(() => expect(app.ws.workspaces.some((x) => x.id === w.id)).toBe(true));
      app.selectWorkspace(w.id);
      return w.id;
    };

    it('sends the workspaceId and no cwd, and still starts agents in a worktree of the subject cwd repo', async () => {
      // Given a subject whose cwd is a project repository, selected in the sidebar
      // When an agent and a shell are started
      // Then both bodies carry workspaceId and no cwd, and only the agent asks for a worktree
      const id = await subject();
      await app.newSession('claude');
      await app.newSession('shell');
      expect(m.bodies['POST /v1/terms']).toEqual([
        { agent: 'claude', cwd: undefined, workspaceId: id, model: undefined, effort: undefined, cols: 120, rows: 32, worktree: true },
        { cwd: undefined, workspaceId: id, cols: 120, rows: 32 },
      ]);
    });

    it('lets an explicit cwd win and sends no workspaceId', async () => {
      // Given a selected subject
      // When a session starts with an explicit cwd
      // Then the body has that cwd and no workspaceId
      await subject();
      await app.newSession('shell', { cwd: '/tmp' });
      expect(m.bodies['POST /v1/terms']).toEqual([{ cwd: '/tmp', cols: 120, rows: 32 }]);
    });
  });

  it('starts a plain shell without agent options or worktree', async () => {
    app.nextProfile = 'p1';
    await app.newSession('shell', { cwd: '/home/alice/Documents/Github/work/webshop', model: 'x' });
    expect(m.bodies['POST /v1/terms']).toEqual([{ cwd: '/home/alice/Documents/Github/work/webshop', workspaceId: 'ws_webshop', cols: 120, rows: 32 }]);
    expect(app.nextProfile).toBe('p1');
  });

  it('uses the next profile once', async () => {
    app.nextProfile = 'economy';
    await app.newSession('claude', { cwd: '/tmp' });
    await app.newSession('claude', { cwd: '/tmp' });
    const sent = m.bodies['POST /v1/terms'].map((b) => (b as { profile?: string }).profile);
    expect(sent).toEqual(['economy', undefined]);
    expect(app.nextProfile).toBeNull();
  });

  it('keeps the next profile when one is given explicitly', async () => {
    app.nextProfile = 'pending';
    await app.newSession('claude', { cwd: '/tmp', profile: 'economy' });
    expect((m.bodies['POST /v1/terms'][0] as { profile?: string }).profile).toBe('economy');
    expect(app.nextProfile).toBe('pending');
  });

  it('starts in no folder in the status sidebar', async () => {
    app.sidebarMode = 'status';
    await app.newSession('claude');
    expect((m.bodies['POST /v1/terms'][0] as { cwd?: string }).cwd).toBeUndefined();
  });

  it('reuses a running terminal and attaches or resumes lines on demand', async () => {
    const running = app.items.find((i) => i.key === 'line:ln_01')!;
    expect(await app.openTerm(running)).toBe('tm_ln_01');
    expect(await app.resumeLine(running)).toBe('tm_ln_01');
    expect(await app.openTerm({ ...running, lineId: undefined, termId: undefined })).toBeNull();
    expect(await app.resumeLine({ ...running, lineId: undefined, termId: undefined })).toBeNull();
    expect(m.calls).toEqual([]);
    const held = app.items.find((i) => i.key === 'line:ln_11')!;
    const id = await app.resumeLine(held);
    expect(m.bodies['POST /v1/lines/ln_11/resume']).toEqual([{ target: 'term' }]);
    expect(app.data.terms.filter((x) => x.id === id)).toHaveLength(1);
  });

  it('asks the daemon to resume a held line whose held terminal is still open', async () => {
    const held = app.items.find((i) => i.key === 'line:ln_11')!;
    const id = await app.resumeLine({ ...held, termId: 'tm_held' });
    expect(m.bodies['POST /v1/lines/ln_11/resume']).toEqual([{ target: 'term' }]);
    expect(id).not.toBe('tm_held');
  });

  it('adds a terminal the daemon has not announced yet', () => {
    const term = { ...app.data.terms[0], id: 'tm_quiet' };
    app.upsertTerm(term);
    expect(app.data.terms.at(-1)).toBe(term);
  });

  it('keeps a terminal added while a refresh is in flight', async () => {
    const term = { ...app.data.terms[0], id: 'tm_during_refresh' };
    const pending = app.refresh();
    app.upsertTerm(term);
    await pending;
    expect(app.data.terms.some((x) => x.id === 'tm_during_refresh')).toBe(true);
  });

  it('prefers the refreshed copy of a terminal added while a refresh is in flight', async () => {
    const fetched = app.data.terms[0];
    const pending = app.refresh();
    app.upsertTerm({ ...fetched, cols: 1, rows: 1 });
    await pending;
    const kept = app.data.terms.filter((x) => x.id === fetched.id);
    expect(kept).toHaveLength(1);
    expect(kept[0].cols).toBe(fetched.cols);
  });

  it('does not add a terminal twice', () => {
    const terms = app.data.terms;
    app.upsertTerm(terms[0]);
    expect(app.data.terms).toBe(terms);
  });

  it('finds the project of a folder, not of a sibling with the same prefix', () => {
    const webshop = '/home/alice/Documents/Github/work/webshop';
    expect(app.projectFor(webshop)).toBe(webshop);
    expect(app.projectFor(`${webshop}/src/x`)).toBe(webshop);
    expect(app.projectFor(`${webshop}2`)).toBeUndefined();
    expect(app.projectFor(undefined)).toBeUndefined();
    expect(app.inProject(`${webshop}/a`)).toBe(true);
    expect(app.inProject('/tmp')).toBe(false);
  });

  it('adds a project once and shows its saved colour', () => {
    const before = app.projects.length;
    settings.projectColors = { '/srv/p': '#00ff00' };
    app.addProject({ path: '/srv/p', name: 'p' } as Project);
    app.addProject({ path: '/srv/p', name: 'p again' } as Project);
    expect(app.projects).toHaveLength(before + 1);
    expect(app.allProjects.find((p) => p.path === '/srv/p')?.color).toBe('#00ff00');
  });

  it('keeps the colour the daemon already knows', () => {
    settings.projectColors = { '/srv/q': '#00ff00' };
    app.addProject({ path: '/srv/q', name: 'q', color: '#123456' } as Project);
    expect(app.allProjects.find((p) => p.path === '/srv/q')?.color).toBe('#123456');
  });

  it('opens a project tab in the focused pane', () => {
    app.selectWorkspace('ws_oslabs');
    app.view = 'kanban';
    app.openProjectTab('worktrees');
    const key = app.selectedKey!;
    expect(key).toContain('/home/alice/Documents/Github/uni/os-labs');
    expect(allTabs(workspace.layout)).toEqual([key]);
    expect(app.view).toBe('agents');
  });

  it('shows source control in the sidebar, reopening a hidden sidebar, and leaves it for a session list', () => {
    app.sidebarGit = false;
    app.sidebarOpen = false;
    app.toggleSourceControl();
    expect([app.sidebarGit, app.sidebarOpen]).toEqual([true, true]);
    expect(settings.sidebarGit).toBe(true);
    app.toggleSourceControl();
    expect(app.sidebarGit).toBe(false);
    app.setSidebarGit(true);
    const mode = app.sidebarMode;
    app.setSidebarMode('status');
    expect(app.sidebarGit).toBe(false);
    expect(app.sidebarMode).toBe('status');
    app.setSidebarMode(mode);
  });

  it('shows a project tab opened from More on the phone', () => {
    ui.layout = 'phone';
    ui.tab = 'more';
    try {
      app.openProjectTab('graph');
      expect(ui.tab).toBe('agents');
      expect(app.selectedKey).toMatch(/^graph:/);
    } finally {
      ui.layout = 'desktop';
      ui.tab = 'agents';
    }
  });

  it('opens no project tab without any project', () => {
    app.projects = [];
    app.selectWorkspace('ws_oslabs');
    app.openProjectTab('graph');
    expect(allTabs(workspace.layout)).toEqual([]);
  });

  it('frees an exited terminal when its tab closes', async () => {
    const term = app.data.terms.find((x) => x.id === 'tm_shell1')!;
    app.data = { ...app.data, terms: app.data.terms.map((x) => (x === term ? { ...x, exited: true } : x)) };
    workspace.layout = openInFocused(openInFocused(EMPTY_LAYOUT, 'term:tm_shell1'), 'term:tm_shell2');
    closeTabAt(0, 'term:tm_shell2', false);
    closeTabAt(0, 'term:tm_shell1');
    await vi.waitFor(() => expect(m.calls).toContain('DELETE /v1/terms/tm_shell1'), { timeout: 5000 });
    expect(m.hits(/^DELETE/)).toEqual(['DELETE /v1/terms/tm_shell1']);
  });

  it('ends the session when its tab closes, and only hides it on Hide tab', async () => {
    const line = app.items.find((i) => i.lineId)!;
    workspace.layout = openInFocused(openInFocused(openInFocused(EMPTY_LAYOUT, line.key), 'term:tm_shell1'), 'term:tm_shell2');
    closeTabAt(0, 'term:tm_shell2', false);
    closeTabAt(0, line.key);
    closeTabAt(0, 'term:tm_shell1');
    await vi.waitFor(() => expect(m.calls).toEqual(expect.arrayContaining([`POST /v1/lines/${line.lineId}/close`, 'POST /v1/terms/tm_shell1/kill'])), { timeout: 5000 });
    expect(m.calls).not.toContain('POST /v1/terms/tm_shell2/kill');
  });

  it('starts a shell in the same folder when an ended terminal tab is reopened', async () => {
    const { runShortcut } = await import('./keyboard');
    const cwd = app.data.terms.find((x) => x.id === 'tm_shell1')!.cwd;
    workspace.layout = openInFocused(EMPTY_LAYOUT, 'term:tm_shell1');
    closeTabAt(0, 'term:tm_shell1');
    await vi.waitFor(() => expect(app.data.terms.some((x) => x.id === 'tm_shell1')).toBe(false), { timeout: 5000 });
    runShortcut({ id: 'reopenTab' });
    await vi.waitFor(() => expect(m.calls).toContain('POST /v1/terms'), { timeout: 5000 });
    expect((m.bodies['POST /v1/terms'].at(-1) as { cwd: string }).cwd).toBe(cwd);
  });

  it('offers Hide tab and Close for a session tab, and only Close for a non-session tab', async () => {
    const { tabMenu } = await import('./tab-edits');
    const line = app.items.find((i) => i.lineId)!;
    workspace.layout = openInFocused(openInFocused(EMPTY_LAYOUT, line.key), 'browser:x');
    const e = {} as MouseEvent;
    const labels = (items: { label: string }[]) => items.map((i) => i.label).filter(Boolean);
    expect(labels(tabMenu(app, e, 0, 'browser:x', false))).toEqual([t('common.close')]);
    const items = tabMenu(app, e, 0, line.key, true);
    expect(labels(items).slice(-2)).toEqual([t('tab.hide'), t('common.close')]);
    items.find((i) => i.label === t('tab.hide'))!.run!();
    expect(allTabs(workspace.layout)).toEqual(['browser:x']);
    await new Promise((r) => setTimeout(r, 0));
    expect(m.hits(/^POST \/v1\/lines/)).toEqual([]);
  });

  it('keeps exited terminals when their tab closes while disconnected', async () => {
    app.data = { ...app.data, terms: app.data.terms.map((x) => ({ ...x, exited: true })) };
    const terms = app.data.terms;
    workspace.layout = openInFocused(EMPTY_LAYOUT, 'term:tm_shell1');
    app.connected = false;
    closeTabAt(0, 'term:tm_shell1');
    await new Promise((r) => setTimeout(r, 0));
    expect(m.calls).toEqual([]);
    app.connected = true;
    expect(app.data.terms).toBe(terms);
  });
});

describe('links', () => {
  beforeEach(async () => {
    await connect();
  });

  it('opens a web page next to the session that showed it', () => {
    app.view = 'kanban';
    expect(app.openPage('localhost:3000', { termId: 'tm_ln_03' })).toBe(true);
    expect(pages.byKey['line:ln_03']?.pages.map((p) => p.url)).toContain('http://localhost:3000');
    expect(app.selectedKey).toBe('line:ln_03');
    expect(app.view).toBe('agents');
  });

  it('refuses pages without a known session or a web URL', () => {
    expect(app.openPage('https://x.dev', {})).toBe(false);
    expect(app.openPage('https://x.dev', { key: 'line:nope' })).toBe(false);
    expect(app.openPage('ftp://x.dev', { key: 'line:ln_03' })).toBe(false);
  });

  it('falls back to the built-in browser view', () => {
    app.openInside('https://x.dev');
    expect(app.browserUrl).toBe('https://x.dev');
    expect(app.view).toBe('browser');
  });

  it('opens links inside Overseer by default', () => {
    app.openUrl('https://x.dev', { key: 'line:ln_03' });
    expect(pages.byKey['line:ln_03']?.pages.map((p) => p.url)).toContain('https://x.dev');
    app.openUrl('file:///tmp/a.txt');
    expect(app.browserUrl).toBe('file:///tmp/a.txt');
    expect(app.view).toBe('browser');
  });

  it('opens links in the system browser when the user chose it', async () => {
    const open = vi.fn();
    vi.stubGlobal('window', { open });
    settings.linkTarget = 'default';
    app.openUrl('https://x.dev');
    await vi.waitFor(() => expect(open).toHaveBeenCalledWith('https://x.dev', '_blank', 'noopener'));
    expect(app.view).toBe('agents');
  });

  it('asks where to open a shift-clicked link and can copy it', async () => {
    app.openUrl('https://x.dev', undefined, { shift: true, at: { clientX: 5, clientY: 6 } });
    await vi.waitFor(() => expect(app.menu).not.toBeNull());
    expect(app.menu).toMatchObject({ x: 5, y: 6, label: 'https://x.dev' });
    const copy = app.menu!.items.find((i) => i.label === t('link.copy'))!;
    copy.run!();
    await vi.waitFor(() => expect(app.toast?.text).toBe(t('link.copied')));
    expect(m.copied).toEqual(['https://x.dev']);
    app.menu!.items.find((i) => i.label === t('link.inOverseer'))!.run!();
    expect(app.view).toBe('browser');
  });
});

describe('toasts', () => {
  it('hides an info toast after a few seconds', () => {
    vi.useFakeTimers();
    app.notify('Saved');
    expect(app.toast).toEqual({ text: 'Saved', kind: 'info', action: undefined });
    vi.advanceTimersByTime(3499);
    expect(app.toast).not.toBeNull();
    vi.advanceTimersByTime(1);
    expect(app.toast).toBeNull();
  });

  it('keeps a toast with an action longer', () => {
    vi.useFakeTimers();
    app.notify('Undo?', 'info', { label: 'Undo', run: () => {} });
    vi.advanceTimersByTime(7999);
    expect(app.toast?.text).toBe('Undo?');
    vi.advanceTimersByTime(1);
    expect(app.toast).toBeNull();
  });

  it('hides the toast and runs its action when the action is clicked', () => {
    const run = vi.fn(() => expect(app.toast).toBeNull());
    app.notify('Issue created', 'info', { label: 'View', run });
    app.runToastAction();
    expect(run).toHaveBeenCalledOnce();
  });

  it('does nothing when the toast has no action or is gone', () => {
    app.notify('Saved');
    app.runToastAction();
    expect(app.toast).toBeNull();
    expect(() => app.runToastAction()).not.toThrow();
  });

  it('restarts the timer for a newer toast', () => {
    vi.useFakeTimers();
    app.notify('one');
    vi.advanceTimersByTime(3000);
    app.notify('two');
    vi.advanceTimersByTime(3000);
    expect(app.toast?.text).toBe('two');
  });

  it('records errors in the notification center', () => {
    app.notify('Disk full', 'error');
    expect(app.toast?.kind).toBe('error');
    expect(center.entries[0]).toMatchObject({ kind: 'error', title: 'Disk full' });
  });

  it('keeps an unfinished progress toast until it is replaced and clamps the fraction', () => {
    vi.useFakeTimers();
    app.notify('x');
    app.progress('Uploading', 0.5);
    vi.advanceTimersByTime(60_000);
    expect(app.toast).toEqual({ text: 'Uploading', kind: 'info', progress: 0.5 });
    app.progress('Uploading', -1);
    expect(app.toast?.progress).toBe(0);
    app.progress('Waiting');
    expect(app.toast?.progress).toBeUndefined();
  });

  it('hides a finished progress toast when nothing replaces it', () => {
    vi.useFakeTimers();
    app.progress('Uploading', 1.5);
    expect(app.toast).toEqual({ text: 'Uploading', kind: 'info', progress: 1 });
    vi.advanceTimersByTime(1500);
    expect(app.toast).toBeNull();
  });

  it('shows notices as toasts and records them', async () => {
    app.notice({ kind: 'error', title: 'Review failed', toast: true });
    await vi.waitFor(() => expect(app.toast).toMatchObject({ text: 'Review failed', kind: 'error' }));
    expect(center.entries[0]).toMatchObject({ title: 'Review failed' });
  });
});
