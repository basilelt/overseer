import { app } from '../state.svelte';
import type { DaemonEvent } from '../types';
import { reviewApi, type ReviewApi } from './api';
import { saveView, savedView } from './map.svelte';
import { reviewCounts } from './counts.svelte';
import * as D from './draft';
import { keepUnchanged, type Mode } from './rows';
import { isSince, loadBase, saveBase } from './base';
import { worktreeOf } from './keys';
import { inRepo } from './repo';
import { upsert, withStatus } from './threads';
import { DaemonError } from '../client';
import { Drafts } from '../drafts';
import { inFlight, single } from '../inflight';
import { onPresence, presence } from '../presence';
import type { Delivery, Related, ReviewDiff, ReviewTarget, SubmitKind, Thread } from './types';
import { readItem, writeItem, KEY_DRAFTS_REVIEW, KEY_REVIEW_AUTO_REFRESH, KEY_REVIEW_MODE } from '../storage';

const MODE_KEY = KEY_REVIEW_MODE;
const AUTO_KEY = KEY_REVIEW_AUTO_REFRESH;
const reviewDrafts = new Drafts<D.Draft | null>(KEY_DRAFTS_REVIEW, (v) => !v);

function loadAuto(): boolean {
  return readItem(AUTO_KEY) !== '0';
}

function saveAuto(on: boolean) {
  writeItem(AUTO_KEY, on ? '1' : '0');
}

function loadMode(): Mode {
  return readItem(MODE_KEY) === 'split' ? 'split' : 'unified';
}

export class ReviewStore {
  diff = $state.raw<ReviewDiff | null>(null);
  threads = $state.raw<Thread[]>([]);
  loading = $state(false);
  error = $state('');
  mode = $state<Mode>(loadMode());
  collapsed = $state<Record<string, boolean>>({});
  expanded = $state<Record<string, string[]>>({});
  fileLines = $state.raw<Record<string, string[]>>({});
  #draft = $state<D.Draft | null>(null);
  selected = $state<string | null>(null);
  delivery = $state.raw<Delivery | null>(null);
  submitting = $state<SubmitKind | null>(null);
  live = $state(false);
  autoRefresh = $state(loadAuto());
  stale = $state(false);
  revision = $state(0);
  missing = $state(false);
  base = $state<string | null>(null);
  related = $state.raw<Related | null>(null);
  unfolded = $state<Record<string, number[]>>({});
  view = $state<'list' | 'map'>(savedView());

  private off: (() => void) | null = null;
  private offPresence: (() => void) | null = null;
  private deferred = false;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private scopeTimer: ReturnType<typeof setTimeout> | undefined;

  target: ReviewTarget;

  private restored = false;
  private readonly draftKey: string;

  get draft(): D.Draft | null {
    return this.#draft;
  }

  set draft(v: D.Draft | null) {
    this.#draft = v;
    reviewDrafts.set(this.draftKey, v && v.body.trim() ? { ...v, selecting: false } : null);
  }

  constructor(target: ReviewTarget) {
    this.target = target;
    this.draftKey = JSON.stringify(target);
    this.#draft = reviewDrafts.get(this.draftKey) ?? null;
    if (target.branch) return;
    const wt = worktreeOf(target, { lines: app.data.lines, tasks: app.data.tasks, terms: app.data.terms });
    this.base = loadBase(wt);
    this.restored = !!this.base;
  }

  get api(): ReviewApi {
    if (!app.client) throw new Error('not connected');
    return reviewApi(app.client);
  }

  get worktree(): string | null {
    return this.diff?.worktree ?? this.target.worktree ?? null;
  }

  get notRepo(): boolean {
    const id = this.target.lineId;
    return !!id && !inRepo(app.data.lines.find((l) => l.line.id === id)?.line);
  }

  get readOnly(): boolean {
    return !!this.target.branch || !!this.diff?.readOnly;
  }

  start() {
    this.off = app.onChange((_p, _n, e) => this.onEvent(e));
    this.offPresence = onPresence((p) => p.visible && this.resume());
    this.load();
    return () => this.stop();
  }

  stop() {
    this.off?.();
    this.off = null;
    this.offPresence?.();
    this.offPresence = null;
    this.deferred = false;
    clearTimeout(this.timer);
    clearTimeout(this.scopeTimer);
    this.live = false;
  }

  private fail(e: unknown) {
    this.missing = e instanceof DaemonError && e.status === 410;
    this.error = String((e as Error)?.message ?? e);
    this.live = false;
  }

  private fetchDiff() {
    return this.api.diff(this.target, this.base ?? undefined);
  }

  async load() {
    if (!app.client || this.notRepo) return;
    this.loading = !this.diff;
    try {
      let [diff, threads] = await Promise.all([this.fetchDiff(), this.target.branch ? Promise.resolve([]) : this.api.threads(this.target)]);
      if (!this.restored && !this.target.branch) {
        this.restored = true;
        const saved = loadBase(diff.worktree);
        if (saved && !this.base) {
          this.base = saved;
          diff = await this.fetchDiff();
        }
      }
      this.diff = { ...diff, files: keepUnchanged(this.diff?.files, diff.files) };
      this.threads = threads;
      this.error = '';
      this.missing = false;
      this.live = true;
      this.stale = false;
      this.revision++;
      reviewCounts.alias(this.target, diff.worktree);
      for (const path of Object.keys(this.expanded)) if (this.expanded[path].length) this.fetchLines(path, true);
      this.loadScope();
    } catch (e) {
      this.diff = null;
      this.fail(e);
    } finally {
      this.loading = false;
    }
  }

  async loadScope() {
    if (!app.client || this.notRepo) return;
    const t = this.worktree ? { worktree: this.worktree } : this.target;
    const related = await this.api.related(t).catch(() => null);
    if (related) this.related = related;
  }

  private scopeSoon() {
    clearTimeout(this.scopeTimer);
    this.scopeTimer = setTimeout(() => this.loadScope(), 2000);
  }

  async setBase(base: string | null, remember = false) {
    this.base = base;
    if (remember && !isSince(base)) saveBase(this.worktree, base);
    await this.reloadDiff();
  }

  setAutoRefresh(on: boolean) {
    this.autoRefresh = on;
    saveAuto(on);
    if (on && this.stale) void this.reloadDiff();
  }

  resetSince() {
    return this.setBase(loadBase(this.worktree));
  }

  async reloadDiff(changed?: string[]) {
    if (!app.client || this.notRepo) return;
    try {
      const diff = await this.fetchDiff();
      this.diff = { ...diff, files: keepUnchanged(this.diff?.files, diff.files) };
      this.error = '';
      this.missing = false;
      this.stale = false;
      this.revision++;
      const paths = changed ?? Object.keys(this.fileLines);
      for (const p of paths) if (this.expanded[p]?.length) this.fetchLines(p, true);
    } catch (e) {
      this.fail(e);
    }
  }

  private listed(wt: string | undefined): boolean {
    return !!wt && !!this.related?.worktrees.some((w) => w.path === wt);
  }

  private mine(wt: string | undefined): boolean {
    if (!wt) return false;
    if (!this.target.branch) return wt === this.worktree;
    return !!this.related?.worktrees.some((w) => w.path === wt && w.branch === this.target.branch);
  }

  private resume() {
    if (!this.deferred) return;
    this.deferred = false;
    void this.reloadDiff();
    void this.loadScope();
  }

  private defer(): boolean {
    if (presence().visible) return false;
    clearTimeout(this.timer);
    clearTimeout(this.scopeTimer);
    this.deferred = true;
    return true;
  }

  onEvent(e: DaemonEvent) {
    const wt = this.worktree;
    switch (e.type) {
      case 'review.changed': {
        const scope = this.listed(e.data?.worktree) || e.data?.worktree === wt;
        const mine = this.mine(e.data?.worktree);
        if ((scope || mine) && this.defer()) return;
        if (scope) this.scopeSoon();
        if (!mine) return;
        if (!this.autoRefresh) {
          this.stale = true;
          return;
        }
        clearTimeout(this.timer);
        this.timer = setTimeout(() => this.reloadDiff(this.target.branch ? undefined : e.data?.files), 300);
        return;
      }
      case 'worktree.created':
      case 'worktree.removed':
      case 'worktree.updated':
        if (!this.defer()) this.scopeSoon();
        return;
      case 'review.thread': {
        const t = e.data?.thread as Thread | undefined;
        if (t && t.worktree === wt) this.threads = upsert(this.threads, t);
        return;
      }
      case 'review.thread_deleted':
        if (e.data?.id) this.threads = this.threads.filter((t) => t.id !== e.data.id);
        return;
      case 'review.delivery': {
        const d = e.data?.delivery as Delivery | undefined;
        if (!d || e.data?.worktree !== wt) return;
        if (this.delivery?.id === d.id) this.delivery = d;
        this.threads = this.threads.map((t) => (t.delivery?.id === d.id ? { ...t, delivery: d } : t));
        return;
      }
    }
  }

  unfold(path: string, hunk: number) {
    const cur = this.unfolded[path] ?? [];
    if (!cur.includes(hunk)) this.unfolded[path] = [...cur, hunk];
  }

  setView(v: 'list' | 'map') {
    this.view = v;
    saveView(v);
  }

  setMode(m: Mode) {
    this.mode = m;
    writeItem(MODE_KEY, m);
  }

  async fetchLines(path: string, force = false) {
    const wt = this.worktree;
    if (!wt || (!force && this.fileLines[path])) return;
    try {
      const f = await this.api.file(wt, path, this.target.branch ? (this.diff?.head ?? undefined) : undefined);
      this.fileLines = { ...this.fileLines, [path]: f.lines };
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  async expand(path: string, gap: string) {
    await this.fetchLines(path);
    const cur = this.expanded[path] ?? [];
    if (!cur.includes(gap)) this.expanded[path] = [...cur, gap];
  }

  async toggleViewed(path: string) {
    const d = this.diff;
    const f = d?.files.find((x) => x.path === path);
    if (!d || !f) return;
    if (this.readOnly) {
      this.collapsed[path] = !this.isCollapsed(path);
      return;
    }
    const viewed = !f.viewed;
    this.diff = { ...d, files: d.files.map((x) => (x.path === path ? { ...x, viewed } : x)) };
    this.collapsed[path] = viewed;
    try {
      await this.api.viewed(d.worktree, path, f.hash, viewed);
    } catch (e) {
      const cur = this.diff;
      if (cur) this.diff = { ...cur, files: cur.files.map((x) => (x.path === path ? { ...x, viewed: !viewed } : x)) };
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  isCollapsed(path: string): boolean {
    const f = this.diff?.files.find((x) => x.path === path);
    return this.collapsed[path] ?? !!f?.viewed;
  }

  private async run<T>(p: Promise<T>): Promise<T | null> {
    try {
      return await p;
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
      return null;
    }
  }

  private put(t: Thread | null) {
    if (!t) return;
    this.threads = upsert(this.threads, t);
    reviewCounts.put(t);
  }

  private adding = false;

  async addDraft() {
    const d = this.draft;
    if (!D.ready(d) || this.adding) return;
    this.adding = true;
    try {
      await this.saveDraft(d!);
    } finally {
      this.adding = false;
    }
  }

  private async saveDraft(d: D.Draft) {
    const target = this.worktree ? { worktree: this.worktree } : this.target;
    const t = await this.run(this.api.createThread({ ...D.toNewThread(d, target), base: this.base ?? undefined }));
    if (t) {
      this.put(t);
      this.draft = null;
    }
  }

  async reply(threadId: string, body: string) {
    if (!body.trim()) return false;
    const t = await this.run(this.api.reply(threadId, body.trim()));
    this.put(t);
    return !!t;
  }

  async edit(commentId: string, body: string) {
    this.put(await this.run(this.api.editComment(commentId, body)));
  }

  async setStatus(t: Thread, action: 'resolve' | 'reopen') {
    this.threads = upsert(this.threads, withStatus(t, action));
    const next = await this.run(this.api.setStatus(t.id, action === 'resolve' ? 'resolved' : 'open'));
    this.put(next ?? t);
  }

  async remove(id: string, threadId: string) {
    const r = await this.run(this.api.remove(id));
    if (!r) return;
    if (r.deleted === threadId) {
      this.threads = this.threads.filter((t) => t.id !== threadId);
      reviewCounts.drop(threadId);
    } else {
      const fresh = await this.run(this.api.threads(this.target));
      if (fresh) this.threads = fresh;
    }
  }

  async apply(t: Thread) {
    if (inFlight(`apply:${t.id}`)) return;
    const next = await this.run(single(`apply:${t.id}`, () => this.api.apply(t.id)));
    if (next) {
      this.put(next);
      this.reloadDiff([t.path]);
    }
  }

  async submit(kind: SubmitKind, body: string) {
    if (this.submitting) return null;
    this.submitting = kind;
    try {
      const target = this.worktree ? { worktree: this.worktree } : this.target;
      const r = await this.run(this.api.submit(target, kind, body));
      if (!r) return null;
      this.delivery = r.delivery;
      for (const t of r.threads) this.put(t);
      return r;
    } finally {
      this.submitting = null;
    }
  }
}
