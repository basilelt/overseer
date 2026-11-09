import type { DaemonEvent } from '../types';
import type { GitApi } from './api';
import type { CommitOptions, ScmResult, ScmStatus, ScmTarget } from './types';

export type ScmAction = 'stage' | 'unstage' | 'discard' | 'commit' | 'fetch' | 'pull' | 'push' | 'switch' | 'merge' | 'stash' | 'pop' | 'revert';

const msg = (e: unknown) => String((e as Error)?.message ?? e);

export function targetKey(t: ScmTarget | null | undefined): string {
  return t?.worktree ? `w:${t.worktree}` : t?.lineId ? `l:${t.lineId}` : '';
}

export function touches(s: ScmStatus | null, e: DaemonEvent): boolean {
  if (!s) return false;
  if (e.type === 'review.changed') return e.data?.worktree === s.worktree;
  if (e.type === 'git.changed' || e.type === 'worktree.updated') return !e.data?.project || e.data.project === s.project;
  return false;
}

export function fileCount(s: ScmStatus | null): number {
  return s ? s.staged.length + s.changes.length + s.conflicts.length : 0;
}

export class Scm {
  status = $state.raw<ScmStatus | null>(null);
  error = $state<string | null>(null);
  busy = $state<ScmAction | null>(null);
  loading = $state(false);
  private key = '';
  private seq = 0;

  constructor(
    private api: () => GitApi | null,
    private notify: (text: string) => void = () => {},
  ) {}

  target: ScmTarget | null = null;

  setTarget(t: ScmTarget | null) {
    const k = targetKey(t);
    if (k === this.key) return;
    this.key = k;
    this.target = t;
    this.status = null;
    this.error = null;
    this.load();
  }

  async load() {
    const api = this.api();
    const t = this.target;
    if (!api || !t || !targetKey(t)) return;
    const n = ++this.seq;
    const key = this.key;
    this.loading = true;
    try {
      const s = await api.status(t);
      if (n !== this.seq || key !== this.key) return;
      this.status = s;
      this.error = null;
    } catch (e) {
      if (n !== this.seq || key !== this.key) return;
      this.error = msg(e);
    } finally {
      if (n === this.seq) this.loading = false;
    }
  }

  private async run(name: ScmAction, call: (api: GitApi, t: ScmTarget) => Promise<ScmResult>): Promise<ScmResult | null> {
    const api = this.api();
    const t = this.status ? { worktree: this.status.worktree } : this.target;
    if (!api || !t || this.busy) return null;
    const key = this.key;
    this.busy = name;
    try {
      const r = await call(api, t);
      if (key === this.key) {
        this.seq++;
        this.status = r.status;
        this.error = null;
      }
      return r;
    } catch (e) {
      this.notify(msg(e));
      return null;
    } finally {
      this.busy = null;
    }
  }

  stage = (files: string[] | 'all') => this.run('stage', (a, t) => a.stage(t, files));
  unstage = (files: string[] | 'all') => this.run('unstage', (a, t) => a.unstage(t, files));
  discard = (files: string[] | 'all') => this.run('discard', (a, t) => a.discard(t, files));
  commit = (o: CommitOptions) => this.run('commit', (a, t) => a.commitChanges(t, o));
  fetch = () => this.run('fetch', (a, t) => a.fetch(t));
  pull = () => this.run('pull', (a, t) => a.pull(t));
  push = () => this.run('push', (a, t) => a.push(t));
  switchTo = (branch: string, create = false) => this.run('switch', (a, t) => a.switchBranch(t, branch, create));
  merge = (rev: string) => this.run('merge', (a, t) => a.merge(t, rev));
  stash = () => this.run('stash', (a, t) => a.stash(t, false));
  pop = () => this.run('pop', (a, t) => a.stash(t, true));
  revert = (sha: string) => this.run('revert', (a, t) => a.revert(t, sha));
}
