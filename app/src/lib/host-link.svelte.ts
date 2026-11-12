import { api } from './api';
import type { ConnectOptions, DaemonClient } from './client';
import { DaemonError } from './client';
import { sidebarItems, stableItems, type SidebarItem } from './grouping';
import { applyEvent, type Snapshot } from './reduce';
import type { ConnState, DaemonEvent, HostTarget, Project } from './types';
import { hostKey } from './window';
import { applyWsEvent, EMPTY_WS, itemWorkspaces, sameMap, type WsData } from './workspaces';

export const EMPTY_SNAPSHOT: Snapshot = { lines: [], terms: [], tasks: [], activity: {} };

export type LinkChange = (link: HostLink, prev: Snapshot, next: Snapshot, e: DaemonEvent) => void;

/**
 * One host of a window. The active one feeds the app store; the others stay connected and keep a
 * light snapshot (sessions, workspaces) from their event stream for their sidebar section.
 */
export class HostLink {
  conn = $state<ConnState>({ phase: 'idle', target: { kind: 'local', label: 'Local' } });
  data = $state.raw<Snapshot>(EMPTY_SNAPSHOT);
  ws = $state.raw<WsData>(EMPTY_WS);
  projects = $state.raw<Project[]>([]);
  /** Connected once since added: requests can go through, even while it reconnects. */
  ready = $state(false);
  watching = $state(false);

  private lastItems: SidebarItem[] = [];
  items = $derived.by(() => (this.watching ? (this.lastItems = stableItems(this.lastItems, sidebarItems(this.data.lines, this.data.terms, this.data.tasks, {}))) : []));
  private lastItemWs = new Map<string, string>();
  itemWs = $derived.by(() => {
    const next = itemWorkspaces(this.items, this.data, this.ws.workspaces, this.projects);
    if (!sameMap(this.lastItemWs, next)) this.lastItemWs = next;
    return this.lastItemWs;
  });

  /** Set by the app while this host is the active one. */
  listener: ((s: ConnState, prev: ConnState) => void) | null = null;
  onChange: LinkChange | null = null;
  /** A background host asks for a password, passphrase or host key answer. */
  onPrompt: (() => void) | null = null;
  private off: (() => void) | null = null;
  private loads = 0;

  constructor(
    public target: HostTarget,
    readonly client: DaemonClient,
  ) {
    this.conn = { phase: 'idle', target };
  }

  get key(): string {
    return hostKey(this.target);
  }

  readonly onState = (s: ConnState) => {
    const prev = this.conn;
    this.conn = s;
    if (s.phase === 'connected') this.ready = true;
    this.listener?.(s, prev);
    if (s.prompt && s.prompt.id !== prev.prompt?.id && !this.listener) this.onPrompt?.();
    if (this.watching && prev.phase === 'reconnecting' && s.phase === 'connected') void this.load();
  };

  async connect(opts: ConnectOptions = {}): Promise<void> {
    await this.client.connect(this.target, this.onState, opts);
    this.ready = true;
  }

  /** Start following this host in the background, from what the app last showed of it. */
  watch(seed?: { data: Snapshot; ws: WsData; projects: Project[] }) {
    if (this.watching) return;
    if (seed) {
      this.data = { ...seed.data, activity: {} };
      this.ws = seed.ws;
      this.projects = seed.projects;
    }
    this.watching = true;
    this.off = this.client.subscribe((e) => this.onEvent(e));
    void this.load();
  }

  unwatch() {
    this.off?.();
    this.off = null;
    this.watching = false;
  }

  onEvent(e: DaemonEvent) {
    if (/^(workspace|folder|group|project)\./.test(e.type)) {
      if (e.data?.project) {
        const p = e.data.project as Project;
        if (!this.projects.some((x) => x.path === p.path)) this.projects = [...this.projects, p];
      } else this.ws = applyWsEvent(this.ws, e);
      return;
    }
    const prev = this.data;
    const next = applyEvent(prev, e);
    if (next === prev) return;
    this.data = next;
    this.onChange?.(this, prev, next, e);
  }

  async load() {
    if (!this.ready || !this.watching) return;
    const seq = ++this.loads;
    const a = api(this.client);
    const ws = async (): Promise<WsData | null> => {
      try {
        const [workspaces, folders, groups] = await Promise.all([a.workspaces(), a.folders(), a.groups()]);
        return { workspaces, folders, groups };
      } catch (e) {
        return e instanceof DaemonError && e.status === 404 ? EMPTY_WS : null;
      }
    };
    const [lines, terms, tasks, projects, w] = await Promise.all([
      a.lines().catch(() => null),
      a.terms().catch(() => null),
      a.tasks().catch(() => null),
      a.projects().catch(() => null),
      ws(),
    ]);
    if (seq !== this.loads || !this.watching) return;
    this.data = { lines: lines ?? this.data.lines, terms: terms ?? this.data.terms, tasks: tasks ?? this.data.tasks, activity: {} };
    if (projects) this.projects = projects;
    if (w) this.ws = w;
  }

  async close() {
    this.unwatch();
    this.listener = null;
    this.onChange = null;
    this.onPrompt = null;
    this.ready = false;
    this.conn = { phase: 'idle', target: this.target };
    await this.client.disconnect().catch(() => {});
  }
}
