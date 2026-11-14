import { activityDelay } from './activity';
import { FALLBACK_AGENTS, isAgentId, isShellId, isShellItem, sessionsAvailable } from './agents';
import { parentOverlay } from './overlay-route';
import { onPresence, presence } from './presence';
import { api, type Api } from './daemon/api';
import { DaemonError, getClient, isMockClient, type DaemonClient } from './daemon/client';
import { groupItems, sidebarItems, stableItems, type SidebarItem } from './session/grouping';
import { applyEvent, deepEqual, mergeFetched, touchedKey, type Snapshot, type Touched } from './reduce';
import { rememberHost, rememberWorkspace, saveSettings, settings, workspaceFor } from './settings/settings.svelte';
import { hostKey, isMainWindow } from './window';
import type { HostLink, LinkChange } from './host/host-link.svelte';
import { normalizeUrl } from './browser';
import { pages } from './session/session-pages.svelte';
import type { EntryKind } from './notify/notifications';
import type { Proposal } from './workspace/organize';
import { dispatch, type NoticeInput } from './notify/os-notify';
import { center } from './notify/notifications.svelte';
import type { Activity, AgentInfo, ConnState, DaemonEvent, Effort, HostTarget, LocalLine, LocalNetworkReport, ProfilesStore, Project, Term, TaskMessage } from './types';
import { isNoRoute } from './localnet';
import { startupTarget } from './conn';
import { rememberShell } from './tabs/closed-tabs';
import { activeKey, allTabs, EMPTY_LAYOUT, forgetLayouts, onTabClosed, openInFocused, update, workspace } from './workspace/workspace.svelte';
import { newTabKey } from './tabs/tabs';
import { aggregate, applyWsEvent, EMPTY_WS, isGhost, itemWorkspaces, resolveTab, sameMap, synthesize, upsert, within, type Aggregate, type WsData } from './workspace/workspaces';
import type { Workspace } from './types';
import type { MenuItem, OpenMenu } from './commands/menu';
import { isWebLink, resolveLink, type LinkFrom, type LinkGesture, type LinkRoute } from './links';
import { t } from './i18n.svelte';
import { browsers, openOutside } from './browsers.svelte';
import { isPhone, isPhoneOr, ui } from './mobile/ui.svelte';
import { feedback } from './feedback/feedback.svelte';
import { tap } from './record/record-state.svelte';

export type View = 'agents' | 'kanban' | 'browser';
export type Overlay = null | 'taskDetail' | 'palette' | 'connect' | 'settings' | 'task' | 'project' | 'review' | 'ports' | 'system' | 'usage' | 'broadcast' | 'holdAll' | 'name' | 'workspaceNew' | 'profiles' | 'shortcuts' | 'file' | 'notifications' | 'closeTab' | 'switchboardImport' | 'uninstall';

export interface FileArg {
  path: string;
  term?: string;
  line?: string;
}

export type SidebarMode = 'workspaces' | 'status';
export const NO_WORKSPACE = '';

export interface ToastAction {
  label: string;
  run: () => void;
}

const LOCAL: HostTarget = { kind: 'local', label: 'Local' };
const EMPTY: Snapshot = { lines: [], terms: [], tasks: [], activity: {} };
// Bounded backoff after a failed workspaces load (e.g. SQLite locked during an upgrade), then stop.
const WS_RETRY_MS = [1000, 2000, 5000, 10000];

function withAdded(terms: Term[], added: Map<string, Term>): Term[] {
  const missing = [...added.values()].filter((t) => !terms.some((x) => x.id === t.id));
  return missing.length ? [...terms, ...missing] : terms;
}

class AppState {
  client = $state.raw<DaemonClient | null>(null);
  link = $state.raw<HostLink | null>(null);
  others = $state.raw<HostLink[]>([]);
  /** @internal */
  hostOrder = $state.raw<string[]>([]);
  hosts = $derived(this.link ? [this.link, ...this.others].sort((a, b) => this.hostOrder.indexOf(a.key) - this.hostOrder.indexOf(b.key)) : this.others);
  /** @internal */
  attachOnReady = false;
  /** @internal Once the window shows a second host: the link of each client, which takes its connection state. */
  linkOf: WeakMap<DaemonClient, HostLink> | null = null;
  /** @internal */
  slots = 0;
  conn = $state<ConnState>({ phase: 'idle', target: LOCAL });
  localNet = $state.raw<LocalNetworkReport | null>(null);
  data = $state.raw<Snapshot>({ lines: [], terms: [], tasks: [], activity: {} });
  projects = $state.raw<Project[]>([]);
  agents = $state.raw<AgentInfo[]>([]);
  private installing = new Map<string, string>();
  profiles = $state.raw<ProfilesStore | null>(null);
  nextProfile = $state<string | null>(null);
  connected = $state(false);
  synced = $state(false);
  mobile = $state(false);
  os = $state('');
  view = $state<View>(initialView());
  overlay = $state<Overlay>(null);
  overlayArg = $state<string | null>(null);
  fileArg = $state<FileArg | null>(null);
  #selectedKey = $state<string | null>(null);
  get selectedKey(): string | null {
    return this.#selectedKey;
  }
  set selectedKey(key: string | null) {
    this.#selectedKey = key;
    if (key) center.markSubjectRead(key);
  }
  panelOpen = $state(true);
  liveReview = $state(false);
  gitWorktree = $state<{ project: string; path: string } | null>(null);
  sidebarOpen = $state(true);
  browserUrl = $state('');
  toast = $state<{ text: string; kind: 'info' | 'error'; action?: ToastAction; progress?: number } | null>(null);

  private lastItems: SidebarItem[] = [];
  items = $derived.by(() => (this.lastItems = stableItems(this.lastItems, sidebarItems(this.data.lines, this.data.terms, this.data.tasks, briefs(this.data.activity)))));
  groups = $derived(groupItems(this.items));
  selected = $derived(this.items.find((i) => i.key === this.selectedKey) ?? null);
  allProjects = $derived(withColors(this.projects, settings.projectColors));

  ws = $state.raw<WsData>(EMPTY_WS);
  wsPersistent = $state(true);
  sidebarMode = $state<SidebarMode>(settings.sidebarMode ?? 'workspaces');
  panelGit = $state(settings.panelGit ?? false);
  workspaceId = $state<string>(NO_WORKSPACE);
  /** @internal */
  following: { id: string; goToWorkspace: boolean } | null = null;
  private pendingTab: 'graph' | 'worktrees' | null = initialTab();
  private wsChosen = false;
  multi = $state.raw<Set<string>>(new Set());
  nameDialog = $state.raw<NameDialog | null>(null);
  broadcastTarget = $state.raw<BroadcastTarget | null>(null);
  menu = $state.raw<OpenMenu | null>(null);
  tabDrop = $state<string | null>(null);
  private lastItemWs = new Map<string, string>();
  itemWs = $derived.by(() => {
    const next = itemWorkspaces(this.items, this.data, this.ws.workspaces, this.projects);
    if (!sameMap(this.lastItemWs, next)) this.lastItemWs = next;
    return this.lastItemWs;
  });
  currentWorkspace = $derived<Workspace | null>(this.ws.workspaces.find((w) => w.id === this.workspaceId) ?? null);
  aggregates = $derived(aggregatesOf(this.items, this.itemWs));

  /** @internal */
  unsubscribe: (() => void) | null = null;
  private listeners = new Set<(prev: Snapshot, next: Snapshot, e: DaemonEvent) => void>();
  private toastTimer: ReturnType<typeof setTimeout> | undefined;
  /** @internal */
  activityTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private wsRetry: ReturnType<typeof setTimeout> | undefined;
  private wsAttempt = 0;
  private refreshing = 0;
  /** @internal */
  added = new Map<string, Term>();
  private eventSeq = 0;
  /** @internal */
  touched: Touched = new Map();

  requireClient(): DaemonClient {
    if (!this.client) throw new Error('not connected');
    return this.client;
  }

  get api(): Api {
    return api(this.requireClient());
  }

  async init() {
    this.client = await getClient();
    const platform = await this.client.platform().catch(() => ({ os: '', mobile: false }));
    this.mobile = platform.mobile;
    this.os = platform.os;
    if (!platform.mobile) void this.askLocalNetwork();
    const hosts = platform.mobile ? await this.client.savedHosts().catch(() => []) : [];
    const handed = platform.mobile || isMainWindow() ? null : await import('./handoff').then((m) => m.takeHandoff());
    const target = handed ?? startupTarget({
      mobile: platform.mobile,
      lastHost: settings.lastHost,
      lastDirect: settings.lastDirect,
      explicitlyDisconnected: settings.explicitlyDisconnected,
      savedHosts: hosts,
      mock: isMockClient(this.client),
      secondary: !isMainWindow(),
    });
    if (!target) {
      this.conn = { phase: 'idle', target: platform.mobile ? { kind: 'direct', label: '' } : LOCAL };
      return this.open('connect');
    }
    if (!platform.mobile) await this.client.dropSlots?.().catch(() => {});
    const restore = !platform.mobile && isMainWindow() && settings.hostReconnect && !settings.explicitlyDisconnected && (settings.windowHosts?.length ?? 0) > 1 ? settings.windowHosts : null;
    await this.connect(target);
    if (restore) void hostOps().then((m) => m.restore(this, restore, target));
    if (this.conn.phase === 'error' && target.kind !== 'local') this.open('connect');
  }

  wake() {
    if (this.mobile) void this.client?.wake().catch(() => {});
  }

  async askLocalNetwork(host?: string) {
    if (settings.localNetworkAsked || !this.client) return;
    try {
      const r = await this.client.localNetworkStatus(host ?? null);
      if (r.status !== 'notApplicable') saveSettings({ localNetworkAsked: true });
    } catch {}
  }

  async probeLocalNetwork(host: string | undefined): Promise<LocalNetworkReport | null> {
    try {
      return (await this.client?.localNetworkStatus(host ?? null)) ?? null;
    } catch {
      return null;
    }
  }

  async connect(target: HostTarget, how: 'replace' | 'add' = 'replace') {
    const key = hostKey(target);
    if (key !== hostKey(this.conn.target) && (this.others.some((l) => l.key === key) || (how === 'add' && (this.connected || this.others.length)))) {
      return hostOps().then((m) => (this.others.some((l) => l.key === key) ? m.switchHost(this, key) : m.addHost(this, target, true)));
    }
    const c = this.client ?? (this.client = await getClient());
    const link = this.linkOf?.get(c);
    if (!link && !this.others.length) this.link = null;
    if (link) {
      const prev = link.key;
      link.ready = false;
      link.target = target;
      if (prev !== key) this.hostOrder = this.hostOrder.map((k) => (k === prev ? key : k));
    }
    this.attachOnReady = false;
    this.unsubscribe?.();
    this.cancelWsRetry();
    this.connected = false;
    this.synced = false;
    this.localNet = null;
    this.conn = { phase: 'connecting', target };
    if (target.kind === 'ssh') await this.askLocalNetwork(target.host);
    try {
      const onState = (st: ConnState) => {
        const l = this.linkOf?.get(c);
        if (l) l.onState(st);
        else if (this.client === c) this.onActiveState(st, this.conn);
      };
      await c.connect(target, onState, { serverBinDir: settings.serverBinDir || undefined });
      if (this.client !== c) return;
      if (link) link.ready = true;
      this.connected = true;
      rememberHost(target, isMainWindow());
      if (link) void hostOps().then((m) => m.saveHosts(this));
      this.unsubscribe = c.subscribe((e) => this.onEvent(e));
      await this.refresh();
    } catch (err) {
      if (this.client !== c) return;
      const error = this.conn.phase === 'error' && this.conn.error ? this.conn.error : String((err as Error)?.message ?? err);
      this.conn = { phase: 'error', target, error, resolved: this.conn.resolved };
      if (target.kind === 'ssh' && isNoRoute(error)) this.localNet = await this.probeLocalNetwork(target.host);
    }
  }

  async restartServer() {
    await this.client?.restartServer();
  }

  /** @internal */
  readonly onActiveState = (s: ConnState, prev: ConnState) => {
    const resumed = this.conn.phase === 'reconnecting' && s.phase === 'connected';
    this.conn = s;
    if (s.phase === 'connected' && this.attachOnReady) {
      this.attachOnReady = false;
      void hostOps().then((m) => m.attach(this));
    } else if (resumed || (prev.phase === 'reconnecting' && s.phase === 'connected' && this.connected)) this.refresh().catch(() => {});
  };

  async disconnect() {
    if (this.others.length && this.link) return this.removeHost(this.link.key);
    if (isMainWindow()) saveSettings({ explicitlyDisconnected: true });
    this.connected = false;
    this.synced = false;
    this.unsubscribe?.();
    this.unsubscribe = null;
    this.cancelWsRetry();
    const target = this.conn.target;
    this.conn = { phase: 'idle', target };
    this.resetHost();
    await this.client?.disconnect();
  }

  /** @internal */
  resetHost() {
    this.data = EMPTY;
    this.agents = [];
    this.installing.clear();
    this.profiles = null;
    this.nextProfile = null;
    this.selectedKey = null;
    this.ws = EMPTY_WS;
    this.workspaceId = NO_WORKSPACE;
    this.wsChosen = false;
    forgetLayouts();
    workspace.layout = EMPTY_LAYOUT;
  }

  addHost(target: HostTarget, activate: boolean): Promise<void> {
    return hostOps().then((m) => m.addHost(this, target, activate));
  }

  switchHost(key: string): Promise<void> {
    return hostOps().then((m) => m.switchHost(this, key));
  }

  removeHost(key: string): Promise<void> {
    return hostOps().then((m) => m.removeHost(this, key));
  }

  moveToWindow(key: string): Promise<void> {
    return hostOps().then((m) => m.moveToWindow(this, key));
  }



  onOther: LinkChange | null = null;




  async refresh() {
    if (!this.connected) return;
    const a = this.api;
    this.loadProfiles();
    this.refreshing++;
    const since = this.eventSeq;
    let complete = true;
    const stale = <T>(v: T) => ((complete = false), v);
    const [lines, terms, tasks, projects, agents, ws] = await Promise.all([
      a.lines().catch(() => stale(this.data.lines)),
      a.terms().catch(() => stale(this.data.terms)),
      a.tasks().catch(() => this.data.tasks),
      a.projects().catch(() => this.projects),
      a.agents().catch((e) => (e instanceof DaemonError && e.status === 404 ? FALLBACK_AGENTS : this.agents.length ? this.agents : FALLBACK_AGENTS)),
      this.loadWs(a),
    ]);
    const cur = this.data;
    this.data = {
      ...cur,
      lines: mergeFetched(lines, cur.lines, 'line', (l) => l.line.id, this.touched, since),
      terms: withAdded(mergeFetched(terms, cur.terms, 'term', (t) => t.id, this.touched, since), this.added),
      tasks: mergeFetched(tasks, cur.tasks, 'task', (t) => t.id, this.touched, since),
    };
    if (--this.refreshing === 0) {
      this.touched.clear();
      this.added.clear();
    }
    this.settleFollow();
    if (complete) {
      this.synced = true;
      pages.useHost(hostKey(this.conn.target));
      pages.prune(new Set(this.items.map((i) => i.key)));
    }
    this.projects = projects;
    this.agents = agents;
    if (this.client && complete) feedback.resume(this.client, terms, (text, kind, action) => this.notify(text, kind, action));
    this.setWs(ws, projects);
    if (!this.selectedKey && this.items.length && !this.currentWorkspace?.tabs.length) this.selectedKey = groupItems(this.scopeItems()).find((g) => g.items.length)?.items[0].key ?? null;
    if (this.pendingTab) {
      const kind = this.pendingTab;
      this.pendingTab = null;
      setTimeout(() => this.openProjectTab(kind), 0);
    }
    this.uploadLegacyProjects();
    const live = lines.filter((l) => l.line.state === 'active' && !this.data.activity[l.line.id] && sessionsAvailable(agents, l.line.agent)).slice(0, 24);
    await Promise.all(live.map((l) => this.loadActivity(l.line.id)));
  }

  private async uploadLegacyProjects() {
    const legacy = settings.extraProjects;
    if (!legacy?.length) return;
    const colors = { ...settings.projectColors };
    const added: Project[] = [];
    for (const p of legacy) {
      const created = await this.api.createProject(p.path).catch(() => null);
      if (!created) continue;
      added.push(created);
      if (p.color) colors[created.path] = p.color;
    }
    saveSettings({ extraProjects: [], projectColors: colors });
    for (const p of added) this.addProject(p);
  }

  private setWs(ws: WsData | null, projects: Project[]) {
    this.ws = ws ?? { ...EMPTY_WS, workspaces: synthesize(projects) };
    this.wsPersistent = !!ws;
    if (!this.workspaceStillExists()) {
      this.workspaceId = this.initialWorkspace();
      this.wsChosen = true;
    }
  }

  /** Null when the daemon has no workspaces API (404); throws on any other error. */
  private async fetchWs(a: Api): Promise<WsData | null> {
    try {
      const [workspaces, folders, groups] = await Promise.all([a.workspaces(), a.folders(), a.groups()]);
      return { workspaces, folders, groups };
    } catch (e) {
      if (e instanceof DaemonError && e.status === 404) return null;
      throw e;
    }
  }

  private async loadWs(a: Api): Promise<WsData | null> {
    this.cancelWsRetry();
    this.wsAttempt = 0;
    try {
      return await this.fetchWs(a);
    } catch {
      this.retryWs();
      return this.wsPersistent ? this.ws : null;
    }
  }

  private retryWs() {
    if (this.wsRetry || this.wsAttempt >= WS_RETRY_MS.length) return;
    this.wsRetry = setTimeout(() => {
      this.wsRetry = undefined;
      void this.reloadWs();
    }, WS_RETRY_MS[this.wsAttempt++]);
  }

  async reloadWs() {
    if (!this.connected) return;
    const attempt = this.wsAttempt;
    try {
      const ws = await this.fetchWs(this.api);
      // A disconnect or a full refresh since this attempt owns the state now.
      if (this.connected && attempt === this.wsAttempt) this.setWs(ws, this.projects);
    } catch {
      if (this.connected && attempt === this.wsAttempt) this.retryWs();
    }
  }

  /** @internal */
  cancelWsRetry() {
    clearTimeout(this.wsRetry);
    this.wsRetry = undefined;
  }

  private workspaceStillExists(): boolean {
    const id = this.workspaceId;
    if (id.startsWith('group:')) return this.ws.groups.some((g) => `group:${g.id}` === id);
    if (!this.wsChosen) return false;
    if (id === NO_WORKSPACE) return true;
    return this.ws.workspaces.some((w) => w.id === id);
  }

  private initialWorkspace(): string {
    const live = this.ws.workspaces.filter((w) => !w.archived && !isGhost(w));
    const want = workspaceFor(this.conn.target);
    const pref = live.find((w) => w.id === want);
    if (pref) return pref.id;
    const busy = live.filter((w) => this.aggregates.get(w.id)?.agents).sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.rank - b.rank);
    return (busy[0] ?? live.find((w) => !w.parentId) ?? live[0])?.id ?? NO_WORKSPACE;
  }

  membersOf(workspaceId: string) {
    return this.items.filter((i) => (this.itemWs.get(i.key) ?? NO_WORKSPACE) === workspaceId);
  }

  scopeItems(): SidebarItem[] {
    if (this.sidebarMode === 'status') return this.items;
    return this.workspaceId.startsWith('group:') ? this.groupItems(this.workspaceId.slice(6)) : this.membersOf(this.workspaceId);
  }

  groupItems(groupId: string) {
    const g = this.ws.groups.find((x) => x.id === groupId);
    if (!g) return [];
    return this.items.filter((i) => g.members.includes(i.key) || (i.termId && g.members.includes(`term:${i.termId}`)));
  }

  selectWorkspace(id: string) {
    this.following = null;
    this.workspaceId = id;
    this.wsChosen = true;
    rememberWorkspace(this.conn.target, id);
    this.view = this.view === 'kanban' ? 'kanban' : 'agents';
  }

  openMenu(at: { clientX: number; clientY: number }, label: string, items: MenuItem[]) {
    this.menu = { items, x: at.clientX, y: at.clientY, label };
  }

  setPanelGit(on: boolean) {
    this.panelGit = on;
    if (on) {
      this.panelOpen = true;
      if (this.view !== 'agents') this.view = 'agents';
    }
    saveSettings({ panelGit: on });
  }

  toggleSourceControl() {
    this.setPanelGit(!(this.panelGit && this.panelOpen && this.view === 'agents'));
  }

  setSidebarMode(mode: SidebarMode) {
    this.sidebarMode = mode;
    saveSettings({ sidebarMode: mode });
  }

  applyWs(next: WsData) {
    this.ws = next;
  }

  upsertWorkspace(w: Workspace) {
    this.ws = { ...this.ws, workspaces: upsert(this.ws.workspaces, w) };
  }

  async patchWorkspace(id: string, patch: Parameters<Api['patchWorkspace']>[1]) {
    const prev = this.ws.workspaces.find((w) => w.id === id);
    if (!prev) return;
    this.upsertWorkspace({ ...prev, ...patch });
    if (!this.wsPersistent) return;
    try {
      this.upsertWorkspace(await this.api.patchWorkspace(id, patch));
    } catch (e) {
      this.upsertWorkspace(prev);
      this.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  renameItem(key: string, raw: string | null): Promise<void> {
    return tabMod ? tabMod.renameItem(this, key, raw) : tabLazy().then((m) => m.renameItem(this, key, raw));
  }

  private itemOf(key: string) {
    const k = resolveTab(key, this.items) ?? key;
    return this.items.find((i) => i.key === k);
  }

  canMoveToWorkspace(key: string): boolean {
    const it = this.itemOf(key);
    return this.wsPersistent && !!(it?.lineId || it?.termId);
  }

  moveToWorkspace(key: string, workspaceId: string): Promise<boolean> {
    return tabMod ? tabMod.moveToWorkspace(this, key, workspaceId) : tabLazy().then((m) => m.moveToWorkspace(this, key, workspaceId));
  }

  private probedProjects = new Set<string>();

  private probeProject(path: unknown) {
    if (typeof path !== 'string' || !path.startsWith('/') || this.projects.some((p) => p.path === path) || this.probedProjects.has(path)) return;
    this.probedProjects.add(path);
    void this.api
      .projects()
      .then((list) => {
        if (this.connected && list.some((p) => !this.projects.some((x) => x.path === p.path))) this.projects = list;
      })
      .catch(() => this.probedProjects.delete(path));
  }

  addProject(p: Project) {
    if (!this.projects.some((x) => x.path === p.path)) this.projects = [...this.projects, p];
  }

  async loadProfiles() {
    if (!this.connected) return;
    try {
      this.profiles = await this.api.profiles();
    } catch {
      this.profiles = null;
    }
  }

  async saveProfiles(patch: Parameters<Api['saveProfiles']>[0]) {
    this.profiles = await this.api.saveProfiles(patch);
  }

  onEvent(e: DaemonEvent) {
    if (e.type === 'profiles.changed') {
      this.loadProfiles();
      return;
    }
    if (/^(workspace|folder|group|project)\./.test(e.type)) {
      if (e.data?.project) this.addProject(e.data.project);
      else this.ws = applyWsEvent(this.ws, e);
      return;
    }
    if (e.type === 'broadcast.delivered') {
      for (const l of this.broadcastListeners) l(e.data);
      return;
    }
    if (e.type === 'review.failed') {
      const d = e.data ?? {};
      void import('./github/strings').then(({ gt }) =>
        this.notice({ kind: 'error', title: gt('startFailed', { error: String(d.error ?? '') }), source: d.repo && d.number ? `${d.repo}#${d.number}` : d.title }),
      );
      return;
    }
    if (e.type === 'organize.proposal') {
      if (e.data?.proposal) for (const l of this.organizeListeners) l(e.data.proposal);
      return;
    }
    if (e.type === 'task.message') {
      if (e.data?.message?.id) for (const l of this.messageListeners) l(e.data.message);
      return;
    }
    if (e.type === 'line.hold_refused') {
      const failed = (e.data?.failed ?? []) as { name: string; detail?: string }[];
      const why = failed.map((c) => c.detail || c.name).join(', ') || (e.data?.error as string | null | undefined) || '';
      this.notify(why ? t('chat.pauseRefused', { reason: why }) : t('chat.pauseCancelled'));
      return;
    }
    if (e.type === 'line.chat') {
      if (e.data?.lineId) for (const l of this.chatListeners) l(e.data);
      return;
    }
    if (e.type === 'task.created') this.probeProject(e.data?.task?.project ?? e.data?.project);
    else if (e.type === 'line.created') this.probeProject(String(e.data?.line?.repo ?? '').replace(/^local:/, ''));
    const prev = this.data;
    const next = applyEvent(prev, e);
    if (this.refreshing) {
      const k = touchedKey(e);
      if (k) this.touched.set(k, ++this.eventSeq);
    }
    if (next === prev && e.type !== 'line.activity' && !/^(review|worktree)\./.test(e.type) && e.type !== 'git.changed') return;
    this.data = next;
    this.settleFollow();
    for (const l of this.listeners) l(prev, next, e);
    if (e.type === 'line.activity' && e.data?.lineId && !e.data.activity) this.scheduleActivity(e.data.lineId);
  }

  private scheduleActivity(lineId: string) {
    if (this.activityTimers.has(lineId)) return;
    this.activityTimers.set(
      lineId,
      setTimeout(() => {
        this.activityTimers.delete(lineId);
        this.loadActivity(lineId);
      }, activityDelay(this.selected?.lineId === lineId, presence().visible)),
    );
  }

  private broadcastListeners = new Set<(d: { id: string; target: string; result: string; reason?: string }) => void>();

  onBroadcast(fn: (d: { id: string; target: string; result: string; reason?: string }) => void): () => void {
    this.broadcastListeners.add(fn);
    return () => this.broadcastListeners.delete(fn);
  }

  private organizeListeners = new Set<(p: Proposal) => void>();

  onOrganize(fn: (p: Proposal) => void): () => void {
    this.organizeListeners.add(fn);
    return () => this.organizeListeners.delete(fn);
  }

  private chatListeners = new Set<(d: { lineId: string; cursor: number }) => void>();

  private messageListeners = new Set<(m: TaskMessage) => void>();

  onTaskMessage(fn: (m: TaskMessage) => void): () => void {
    this.messageListeners.add(fn);
    return () => this.messageListeners.delete(fn);
  }

  onChat(fn: (d: { lineId: string; cursor: number }) => void): () => void {
    this.chatListeners.add(fn);
    return () => this.chatListeners.delete(fn);
  }

  onChange(fn: (prev: Snapshot, next: Snapshot, e: DaemonEvent) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  async loadActivity(lineId: string) {
    if (!this.connected) return null;
    const line = this.data.lines.find((l) => l.line.id === lineId);
    if (line && !sessionsAvailable(this.agents, line.line.agent)) return null;
    const act = await this.api.activity(lineId).catch(() => null);
    if (!act || act.transcript === false) return null;
    const stale = (l: LocalLine) => l.line.id === lineId && (l.local?.needsInput !== act.needsInput || (act.busy !== undefined && l.local?.busy !== act.busy));
    const lines = this.data.lines.some(stale) ? this.data.lines.map((l) => (stale(l) ? { ...l, local: { ...l.local, needsInput: act.needsInput, busy: act.busy ?? l.local?.busy } } : l)) : this.data.lines;
    const prev = this.data.activity[lineId];
    if (lines === this.data.lines && prev && deepEqual(prev, act)) return prev;
    this.data = { ...this.data, lines, activity: { ...this.data.activity, [lineId]: act } };
    return act;
  }

  async openTerm(item: SidebarItem): Promise<string | null> {
    if (item.termId) return item.termId;
    if (!item.lineId) return null;
    const term = await this.api.attach(item.lineId);
    this.upsertTerm(term);
    return term.id;
  }

  async resumeLine(item: SidebarItem): Promise<string | null> {
    const held = item.lineId && this.data.lines.find((l) => l.line.id === item.lineId)?.line.state === 'held';
    if (item.termId && !held) return item.termId;
    if (!item.lineId) return null;
    const term = await this.api.resume(item.lineId);
    this.upsertTerm(term);
    return term.id;
  }

  async installAgent(id: string): Promise<Term> {
    const term = await this.newSession('shell', { install: id, worktree: false });
    this.installing.set(term.id, id);
    void this.installsEnded(this.data.terms);
    return term;
  }

  async installsEnded(terms: Term[]): Promise<void> {
    if (!this.installing.size) return;
    const ended = terms.filter((x) => x.exited && this.installing.has(x.id)).map((x) => ({ id: this.installing.get(x.id)!, ok: x.exitCode === 0 }));
    if (!ended.length) return;
    for (const x of terms) if (x.exited) this.installing.delete(x.id);
    try {
      this.agents = await this.api.rescanAgents();
    } catch (e) {
      this.notify(String((e as Error)?.message ?? e), 'error');
      return;
    }
    for (const e of ended) {
      const a = this.agents.find((x) => x.id === e.id);
      if (a) this.notify(t(a.installed ? 'agents.installed' : e.ok ? 'agents.installNotFound' : 'agents.installFailed', { name: a.name }), a.installed ? 'info' : 'error');
    }
  }

  async newSession(agent: string, opts: { cwd?: string; model?: string; effort?: Effort; worktree?: boolean; profile?: string; install?: string } = {}): Promise<Term> {
    const scoped = this.sidebarMode === 'workspaces' ? this.currentWorkspace : null;
    // A subject has no directory: the daemon starts the term in its cwd and assigns it.
    // A repo workspace is assigned too: the term runs in a worktree outside its path until it reports a line.
    const subject = scoped && (!opts.cwd || (scoped.kind !== 'subject' && !!scoped.path && within(opts.cwd, scoped.path))) ? scoped.id : undefined;
    const cwd = opts.cwd ?? (scoped?.kind === 'subject' ? undefined : (scoped?.cwd ?? scoped?.path)) ?? undefined;
    const worktree = (opts.worktree ?? (isAgentId(agent) && !settings.sessionsInCheckout)) && this.inProject(cwd ?? (subject ? (scoped?.cwd ?? undefined) : undefined));
    const size = { cols: 120, rows: 32, ...(worktree ? { worktree: true } : {}) };
    const profile = isShellId(agent) ? undefined : (opts.profile ?? this.nextProfile ?? undefined);
    const term = await this.api.createTerm(isShellId(agent) ? { cwd, ...(opts.install ? { install: opts.install } : {}), ...(subject ? { workspaceId: subject } : {}), ...size } : { agent, cwd, ...(subject ? { workspaceId: subject } : {}), model: opts.model || undefined, effort: opts.effort, ...(profile ? { profile } : {}), ...size });
    if (profile && !opts.profile) this.nextProfile = null;
    this.followSession(term, false);
    return term;
  }

  followSession(term: Term, goToWorkspace = true) {
    this.upsertTerm(term);
    this.following = { id: term.id, goToWorkspace };
    this.focusKey(`term:${term.id}`);
  }

  private focusKey(key: string) {
    if (this.following?.goToWorkspace) this.select(key);
    else {
      this.selectedKey = key;
      this.view = 'agents';
    }
  }

  private settleFollow() {
    const id = this.following?.id;
    if (!id) return;
    const item = this.items.find((i) => i.termId === id);
    if (!item) {
      if (!this.data.terms.some((t) => t.id === id && !t.exited)) this.following = null;
      return;
    }
    const cur = this.selectedKey;
    if (cur !== item.key && (cur === null || cur === `term:${id}` || !this.items.some((i) => i.key === cur))) this.focusKey(item.key);
    if (item.lineId) this.following = null;
  }

  openProjectTab(kind: 'graph' | 'worktrees', project?: string) {
    const p = project ?? this.projectFor(this.sidebarMode === 'workspaces' ? (this.currentWorkspace?.path ?? undefined) : undefined) ?? this.projects[0]?.path;
    if (!p) return;
    const key = newTabKey(kind, p);
    this.view = 'agents';
    if (isPhone()) ui.tab = 'agents';
    update((l) => openInFocused(l, key));
    this.selectedKey = key;
  }

  inProject(cwd: string | undefined): boolean {
    return !!this.projectFor(cwd);
  }

  projectFor(cwd: string | undefined): string | undefined {
    if (!cwd) return undefined;
    return this.projects.find((p) => cwd === p.path || cwd.startsWith(`${p.path}/`))?.path;
  }

  upsertTerm(term: Term) {
    if (this.refreshing) this.added.set(term.id, term);
    if (this.data.terms.some((t) => t.id === term.id)) return;
    this.data = { ...this.data, terms: [...this.data.terms, term] };
  }

  select(key: string | null) {
    if (key && this.sidebarMode === 'workspaces' && !allTabs(workspace.layout).includes(key)) {
      const ws = this.itemWs.get(key) ?? (key.includes(':') && this.items.some((i) => i.key === key) ? NO_WORKSPACE : this.workspaceId);
      if (ws !== this.workspaceId) this.workspaceId = ws;
    }
    this.selectedKey = key;
    this.view = 'agents';
  }

  open(overlay: Overlay, arg: string | FileArg | null = null) {
    this.overlay = overlay;
    this.overlayArg = typeof arg === 'string' ? arg : null;
    this.fileArg = arg && typeof arg === 'object' ? arg : null;
  }

  closeOverlay() {
    this.open(parentOverlay(this.overlay));
  }

  openPage(url: string, from: { key?: string | null; termId?: string }): boolean {
    const key = from.key ?? (from.termId ? this.items.find((i) => i.termId === from.termId)?.key : undefined);
    const web = normalizeUrl(url);
    if (!key || !/^https?:\/\//i.test(web) || !this.items.some((i) => i.key === key)) return false;
    pages.useHost(hostKey(this.conn.target));
    pages.attach(key, web);
    if (this.view !== 'agents' || activeKey(workspace.layout) !== key) this.select(key);
    return true;
  }

  openUrl(url: string, from?: LinkFrom, g: LinkGesture & { at?: { clientX: number; clientY: number }; inTerminal?: boolean } = {}) {
    const route: LinkRoute = isWebLink(url) ? resolveLink(settings, g, { phone: this.phoneLinks(), inTerminal: g.inTerminal, browsers: browsers.list }) : { kind: 'overseer' };
    if (route.kind === 'ask') return this.linkMenu(g.at, url, from);
    if (route.kind === 'overseer') return this.openInside(url, from);
    openOutside(url, route).catch((e) => this.notify(String((e as Error)?.message ?? e), 'error'));
  }

  openInside(url: string, from?: LinkFrom) {
    if (from && this.openPage(url, from)) return;
    this.browserUrl = url;
    this.view = 'browser';
  }

  linkMenu(at: { clientX: number; clientY: number } | undefined, url: string, from?: LinkFrom) {
    const pos = at ?? { clientX: innerWidth / 2 - 100, clientY: innerHeight / 3 };
    const fail = (e: unknown) => this.notify(String((e as Error)?.message ?? e), 'error');
    void Promise.all([browsers.load(!this.phoneLinks()), import('./terminal/linkmenu')]).then(([list, { linkItems }]) =>
      this.openMenu(
        pos,
        url,
        linkItems(list, {
          inside: () => this.openInside(url, from),
          outside: (r) => void openOutside(url, r).catch(fail),
          copy: () => void this.client?.copyText(url).then(() => this.notify(t('link.copied')), fail),
        }),
      ),
    );
  }

  private phoneLinks(): boolean {
    return isPhoneOr(this.mobile);
  }

  notify(text: string, kind: 'info' | 'error' = 'info', action?: ToastAction, entry: EntryKind = 'error') {
    if (kind === 'error') tap.note?.('toast', text);
    if (kind === 'error') dispatch(this, { kind: entry, title: text, coalesce: `error:${text}`, toast: false }, () => {});
    this.show(text, kind, action);
  }

  private show(text: string, kind: 'info' | 'error', action?: ToastAction) {
    this.toast = { text, kind, action };
    clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => (this.toast = null), action ? 8000 : 3500);
  }

  runToastAction() {
    const action = this.toast?.action;
    this.toast = null;
    clearTimeout(this.toastTimer);
    action?.run();
  }

  notice(input: NoticeInput, action?: ToastAction) {
    dispatch(this, input, (text, kind, a) => this.show(text, kind, a), action);
  }

  progress(text: string, fraction?: number) {
    const progress = fraction === undefined ? undefined : Math.max(0, Math.min(1, fraction));
    this.toast = { text, kind: 'info', progress };
    clearTimeout(this.toastTimer);
    if (progress === 1) this.toastTimer = setTimeout(() => (this.toast = null), 1500);
  }
}

export interface NameDialog {
  title: string;
  label: string;
  value: string;
  submit: (name: string) => Promise<unknown> | unknown;
}

export type BroadcastTarget = { kind: 'workspace'; id: string } | { kind: 'group'; id: string } | { kind: 'items'; keys: string[] };

function aggregatesOf(items: SidebarItem[], itemWs: Map<string, string>): Map<string, Aggregate> {
  const by = new Map<string, SidebarItem[]>();
  for (const i of items) {
    const w = itemWs.get(i.key) ?? NO_WORKSPACE;
    const list = by.get(w);
    if (list) list.push(i);
    else by.set(w, [i]);
  }
  return new Map([...by].map(([w, list]) => [w, aggregate(list)]));
}

function initialTab(): 'graph' | 'worktrees' | null {
  const hash = globalThis.location?.hash.slice(1);
  return hash === 'graph' || hash === 'worktrees' ? hash : null;
}

function initialView(): View {
  const hash = globalThis.location?.hash.slice(1);
  return hash === 'kanban' || hash === 'browser' ? hash : 'agents';
}

function briefs(activity: Snapshot['activity']) {
  const out: Record<string, { goal?: string; status?: string }> = {};
  for (const [id, a] of Object.entries(activity)) out[id] = { goal: a.goal, status: statusLine(a) };
  return out;
}

export function statusLine(a: Activity): string | undefined {
  const last = a.workLog.at(-1);
  if (a.needsInput || !last) return a.lastAssistantText.split('\n')[0].replace(/`/g, '') || undefined;
  return last.label;
}

export function projectName(projects: Project[], path: string): string {
  return projects.find((p) => p.path === path)?.name ?? path.split('/').filter(Boolean).pop() ?? path;
}

function withColors(projects: Project[], colors: Record<string, string>): Project[] {
  return projects.map((p) => (colors[p.path] && !p.color ? { ...p, color: colors[p.path] } : p));
}

export const app = new AppState();
onPresence((p) => p.focused && app.reloadWs());

/** Adding, switching and removing the hosts of a window, loaded once a window gets a second host. */
const hostOps = () => import('./host/host-ops');

/** Tab rename, move and full-title code, kept out of the startup bundle. */
let tabMod: typeof import('./tabs/tab-lazy') | null = null;

export function tabLazy() {
  return import('./tabs/tab-lazy').then((m) => (tabMod = m));
}

onTabClosed((key, end) => {
  const item = app.items.find((i) => i.key === key);
  const cwd = item && isShellItem(item) ? app.data.terms.find((t) => t.id === item.termId)?.cwd : undefined;
  if (cwd) rememberShell(key, cwd);
  if (app.connected) void tabLazy().then((m) => m.tabClosed(app, key, end));
});

app.onChange((prev, next) => {
  if (prev.terms !== next.terms) void app.installsEnded(next.terms);
  if (prev.terms !== next.terms && next.terms.some((t) => t.exited)) void tabLazy().then((m) => m.closeCleanExits(app, prev.terms, next.terms));
});
