import { untrack } from 'svelte';
import { systemApi } from './api';
import { DaemonError, type DaemonClient } from '../client';
import { ui } from '../mobile/ui.svelte';
import { mergeHistory } from './format';
import type { SysPoint, SysResponse, SysSample } from './types';

export type Mode = 'chip' | 'live';

export const CHIP_MS = 15_000;
export const RETRY_MS = 10_000;

type Client = Pick<DaemonClient, 'request'>;

export function pageVisible(doc: Pick<Document, 'visibilityState' | 'hasFocus'> | undefined = globalThis.document, mobile = ui.mobileOs): boolean {
  if (!doc) return true;
  if (doc.visibilityState === 'hidden') return false;
  return mobile || typeof doc.hasFocus !== 'function' || doc.hasFocus();
}

export class SystemMonitor {
  current = $state.raw<SysSample | null>(null);
  history = $state.raw<SysPoint[]>([]);
  intervalMs = $state(2000);
  unavailable = $state(false);
  visible = $state(true);

  private session: string | null = null;
  private client: Client | null = null;
  private users = new Map<number, Mode>();
  private seq = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private inflight = false;
  private generation = 0;
  private readonly onVisibility = () => {
    const was = this.visible;
    this.visible = this.isVisible();
    if (this.visible && !was) this.schedule(0);
    if (!this.visible) this.clear();
  };

  constructor(
    private isVisible: () => boolean = () => pageVisible(),
    private now: () => number = Date.now,
  ) {}

  get live(): boolean {
    return [...this.users.values()].includes('live');
  }

  get active(): boolean {
    return this.users.size > 0 && this.visible && !this.unavailable;
  }

  use(session: string, client: Client, mode: Mode): () => void {
    return untrack(() => this.attach(session, client, mode));
  }

  private attach(session: string, client: Client, mode: Mode): () => void {
    if (session !== this.session) this.reset(session, client);
    const id = ++this.seq;
    if (!this.users.size) this.listen(true);
    this.users.set(id, mode);
    this.visible = this.isVisible();
    this.schedule(mode === 'live' || !this.current ? 0 : this.period());
    return () =>
      untrack(() => {
        if (!this.users.delete(id)) return;
        if (!this.users.size) {
          this.listen(false);
          this.clear();
        } else this.schedule(this.period());
      });
  }

  private reset(session: string, client: Client) {
    this.clear();
    this.generation++;
    this.session = session;
    this.client = client;
    this.current = null;
    this.history = [];
    this.unavailable = false;
    this.inflight = false;
  }

  private listen(on: boolean) {
    if (typeof document === 'undefined') return;
    const m = on ? 'addEventListener' : 'removeEventListener';
    document[m]('visibilitychange', this.onVisibility);
    globalThis[m]?.('focus', this.onVisibility);
    globalThis[m]?.('blur', this.onVisibility);
  }

  private period(): number {
    return this.live ? this.intervalMs : CHIP_MS;
  }

  private clear() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  private schedule(delay: number) {
    this.clear();
    if (!this.active) return;
    this.timer = setTimeout(() => void this.tick(), delay);
  }

  async tick(): Promise<void> {
    this.timer = null;
    if (this.inflight || !this.client || !this.active) return;
    const gen = this.generation;
    const last = this.history.length ? this.history[this.history.length - 1].t : null;
    this.inflight = true;
    let next = this.period();
    try {
      const r = await systemApi(this.client).system(last);
      if (gen !== this.generation) return;
      if (r.intervalMs > 0) this.intervalMs = r.intervalMs;
      if (r.current) this.current = r.current;
      this.history = mergeHistory(this.history, r.history ?? [], r.current?.t ?? this.now());
      next = this.period();
    } catch (e) {
      if (gen !== this.generation) return;
      if (e instanceof DaemonError && (e.status === 404 || e.status === 501)) {
        this.unavailable = true;
        return;
      }
      next = RETRY_MS;
    } finally {
      if (gen === this.generation) this.inflight = false;
    }
    if (gen === this.generation) this.schedule(next);
  }
}

export const system = new SystemMonitor();
