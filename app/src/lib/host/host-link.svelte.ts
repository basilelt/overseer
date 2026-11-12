import type { ConnectOptions, DaemonClient } from '../daemon/client';
import type { Snapshot } from '../reduce';
import type { ConnState, DaemonEvent, HostTarget, Project } from '../types';
import { hostKey } from '../window';
import { EMPTY_WS, type WsData } from '../workspace/workspaces';

export const EMPTY_SNAPSHOT: Snapshot = { lines: [], terms: [], tasks: [], activity: {} };

export type LinkChange = (link: HostLink, prev: Snapshot, next: Snapshot, e: DaemonEvent) => void;

/**
 * One host of a window. The active one feeds the app store; the others stay connected and keep a
 * light snapshot (sessions, workspaces) from their event stream, filled by `host-watch.ts`.
 */
export class HostLink {
  conn = $state<ConnState>({ phase: 'idle', target: { kind: 'local', label: 'Local' } });
  data = $state.raw<Snapshot>(EMPTY_SNAPSHOT);
  ws = $state.raw<WsData>(EMPTY_WS);
  projects = $state.raw<Project[]>([]);
  /** Connected once since added: requests can go through, even while it reconnects. */
  ready = $state(false);
  watching = $state(false);

  /** Set by the app while this host is the active one. */
  listener: ((s: ConnState, prev: ConnState) => void) | null = null;
  onChange: LinkChange | null = null;
  /** A background host asks for a password, passphrase or host key answer. */
  onPrompt: (() => void) | null = null;
  /** Set while watched: stops the event stream, and reloads after a reconnect. */
  off: (() => void) | null = null;
  reload: (() => void) | null = null;

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
    if (this.watching && prev.phase === 'reconnecting' && s.phase === 'connected') this.reload?.();
  };

  async connect(opts: ConnectOptions = {}): Promise<void> {
    await this.client.connect(this.target, this.onState, opts);
    this.ready = true;
  }

  unwatch() {
    this.off?.();
    this.off = null;
    this.reload = null;
    this.watching = false;
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
