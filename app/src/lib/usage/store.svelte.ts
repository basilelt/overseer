import { untrack } from 'svelte';
import { DaemonError, type DaemonClient } from '../client';
import { ui } from '../mobile/ui.svelte';
import { active, onPresence } from '../presence';
import type { DaemonEvent } from '../types';
import { requestApi } from '../api';
import { tzOffset } from './format';
import type { UsageBy, UsageRange, UsageResponse } from './types';

type Client = Pick<DaemonClient, 'request'>;
type Subscribe = (fn: (e: DaemonEvent) => void) => () => void;

export const DEBOUNCE_MS = 800;
export const RETRY_MS = 30_000;
export const STALE_MS = 5 * 60_000;
export const PANEL_MIN_MS = 2_000;
export const CHIP_MIN_MS = 10_000;

export function pageVisible(doc: Pick<Document, 'visibilityState'> | undefined = globalThis.document): boolean {
  return !doc || doc.visibilityState !== 'hidden';
}

export interface Query {
  range: UsageRange;
  by: UsageBy;
  line?: string;
  claudeLimits?: boolean;
}

export const RESET_MARGIN_MS = 2_000;

export function limitsRefreshIn(r: UsageResponse, now: number): number | null {
  let next: number | null = null;
  for (const a of r.agents)
    for (const w of a.limits?.windows ?? []) {
      if (w.stale) continue;
      const t = w.resetsAt ? Date.parse(w.resetsAt) - now : NaN;
      const d = Number.isFinite(t) && t > 0 ? Math.min(STALE_MS, t + RESET_MARGIN_MS) : STALE_MS;
      next = next === null ? d : Math.min(next, d);
    }
  return next;
}

export class UsageFeed {
  data = $state.raw<UsageResponse | null>(null);
  unavailable = $state(false);
  visible = $state(true);
  loading = $state(false);
  minInterval = PANEL_MIN_MS;

  private session: string | null = null;
  private client: Client | null = null;
  private query: Query | null = null;
  private off: (() => void) | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private due = 0;
  private timed = false;
  private live = true;
  private offPresence: (() => void) | null = null;
  private dirty = false;
  private fetchedAt = 0;
  private generation = 0;
  private readonly onVisibility = () => {
    const was = this.visible;
    const wasLive = this.live;
    this.visible = this.isVisible();
    this.live = this.visible && this.isActive();
    if (!this.live && this.timed) {
      this.clear();
      this.dirty = true;
    }
    const back = (this.visible && !was) || (this.live && !wasLive);
    if (back && (this.dirty || this.now() - this.fetchedAt > STALE_MS)) this.schedule(0);
  };

  constructor(
    private isVisible: () => boolean = () => pageVisible(),
    private now: () => number = Date.now,
    private tz: () => number = () => tzOffset(),
    private isActive: () => boolean = active,
  ) {}

  use(session: string, client: Client, subscribe: Subscribe, query: Query): () => void {
    return untrack(() => {
      const same = session === this.session && this.query && JSON.stringify(query) === JSON.stringify(this.query);
      this.stop();
      if (session !== this.session) {
        this.data = null;
        this.unavailable = false;
      }
      this.generation++;
      this.session = session;
      this.client = client;
      this.query = query;
      this.visible = this.isVisible();
      this.live = this.visible && this.isActive();
      this.off = subscribe((e) => {
        if (e.type === 'usage.updated') this.changed();
      });
      this.listen(true);
      if (!same || !this.data || this.dirty) this.schedule(0);
      return () => untrack(() => this.stop());
    });
  }

  private stop() {
    this.clear();
    this.off?.();
    this.off = null;
    this.listen(false);
    this.query = null;
  }

  private listen(on: boolean) {
    this.offPresence?.();
    this.offPresence = on ? onPresence(this.onVisibility) : null;
    if (typeof document === 'undefined') return;
    const m = on ? 'addEventListener' : 'removeEventListener';
    document[m]('visibilitychange', this.onVisibility);
    if (!ui.mobileOs) globalThis[m]?.('focus', this.onVisibility);
  }

  changed() {
    this.dirty = true;
    if (!this.visible) return;
    const delay = Math.max(DEBOUNCE_MS, this.fetchedAt + this.minInterval - this.now());
    if (this.timer && this.due <= this.now() + delay) return;
    this.schedule(delay);
  }

  private clear() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.timed = false;
  }

  private schedule(delay: number, timed = false) {
    this.clear();
    if (!this.query || this.unavailable) return;
    this.due = this.now() + delay;
    this.timed = timed;
    this.timer = setTimeout(() => void this.fetch(), delay);
  }

  async fetch(): Promise<void> {
    this.timer = null;
    this.timed = false;
    const q = this.query;
    if (!q || !this.client || !this.visible) return;
    const gen = this.generation;
    this.dirty = false;
    this.loading = true;
    try {
      const r = await requestApi(this.client).usage(q.range, q.by, this.tz(), q.line, q.claudeLimits);
      if (gen !== this.generation) return;
      this.data = r;
      this.fetchedAt = this.now();
      if (r.indexing) this.schedule(3000);
      else if (this.live) {
        const next = limitsRefreshIn(r, this.fetchedAt);
        if (next !== null) this.schedule(next, true);
      }
    } catch (e) {
      if (gen !== this.generation) return;
      if (e instanceof DaemonError && (e.status === 404 || e.status === 501)) {
        if (!q.line || e.status === 501) this.unavailable = true;
        else this.data = null;
        return;
      }
      this.dirty = true;
      this.schedule(RETRY_MS);
    } finally {
      if (gen === this.generation) this.loading = false;
    }
  }
}

export const chipFeed = new UsageFeed();
chipFeed.minInterval = CHIP_MIN_MS;
