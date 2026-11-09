import { DaemonError } from '../client';
import type { EventType, LocalLine } from '../types';
import type { ScmCommit, ScmFile, ScmResult, ScmStatus } from './types';

export interface ScmHost {
  lines: LocalLine[];
  emit(type: EventType, data: unknown): void;
}

interface Tree {
  branch: string | null;
  head: string;
  upstream: string | null;
  remote: boolean;
  base: string;
  staged: ScmFile[];
  changes: ScmFile[];
  conflicts: ScmFile[];
  outgoing: ScmCommit[];
  incoming: ScmCommit[];
  stashes: { staged: ScmFile[]; changes: ScmFile[] }[];
  branches: string[];
}

function hash(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}

function sha(seed: string): string {
  let out = '';
  let h = hash(seed);
  while (out.length < 40) {
    h = Math.imul(h ^ (h >>> 15), 0x2c1b3c6d) >>> 0;
    out += h.toString(16).padStart(8, '0');
  }
  return out.slice(0, 40);
}

const BRANCH = /^(?!-)(?!.*\.\.)(?!.*\/\/)(?!.*@\{)(?!.*\.lock$)(?!.*\/$)(?!.*\.$)[^\s~^:?*[\\\x00-\x1f\x7f]+$/;

export function validPath(p: unknown): p is string {
  if (typeof p !== 'string' || !p || p.includes('\0') || p.startsWith('/')) return false;
  return !p.split('/').some((c) => c === '..');
}

export class MockScm {
  private trees = new Map<string, Tree>();

  constructor(
    private host: ScmHost,
    private projectOf: (wt: string) => string,
    private hasCommit: (project: string, rev: string) => boolean = () => false,
  ) {}

  private known(wt: string, t: Tree, rev: string): boolean {
    const r = rev.toLowerCase();
    const mine = [t.head, ...t.outgoing.map((c) => c.sha), ...t.incoming.map((c) => c.sha)];
    return mine.some((s) => s.startsWith(r)) || this.hasCommit(this.projectOf(wt), r);
  }

  route(method: string, path: string, q: URLSearchParams, body: any): unknown {
    const m = path.match(/^\/v1\/git\/([a-z]+)$/);
    if (!m) return undefined;
    const op = m[1];
    if (method === 'GET' && op === 'status') return this.status(this.resolve(q.get('worktree'), q.get('line')));
    if (method !== 'POST') return undefined;
    const b = body ?? {};
    const route = (this as unknown as Record<string, ((wt: string, b: any) => ScmResult) | undefined>)[`op_${op}`];
    if (!route) return undefined;
    const wt = this.resolve(typeof b.worktree === 'string' && b.worktree ? b.worktree : null, typeof b.lineId === 'string' && b.lineId ? b.lineId : null);
    return route.call(this, wt, b);
  }

  private resolve(worktree: string | null, line: string | null): string {
    if (worktree) {
      if (!worktree.startsWith('/')) throw new DaemonError(400, `${worktree} is not an absolute path`);
      return worktree.replace(/\/+$/, '') || '/';
    }
    if (line) {
      const l = this.host.lines.find((x) => x.line.id === line);
      if (!l) throw new DaemonError(404, `no line matches '${line}'`);
      if (!l.line.worktreePath) throw new DaemonError(410, `the worktree folder ${l.line.worktreePath ?? ''} no longer exists`);
      if (l.line.repo === 'unknown') throw new DaemonError(400, `${l.line.worktreePath} is not in a git worktree`);
      return l.line.worktreePath;
    }
    throw new DaemonError(400, 'worktree or lineId required');
  }

  private tree(wt: string): Tree {
    let t = this.trees.get(wt);
    if (t) return t;
    const line = this.host.lines.find((l) => l.line.worktreePath === wt);
    const project = this.projectOf(wt);
    const branch = line?.line.branch ?? (wt === project ? 'main' : wt.split('/').pop()!.replace(/^(feat|fix|chore)-/, '$1/'));
    const n = hash(wt);
    const files = ['src/term.rs', 'src/handlers.rs', 'src/model.rs', 'app/src/lib/state.svelte.ts', 'README.md', 'tests/resize.rs'];
    const pickF = (i: number) => files[(n + i) % files.length];
    const main = branch === 'main';
    t = {
      branch,
      head: sha(`${wt}:head`),
      upstream: main || n % 3 ? `origin/${branch}` : null,
      remote: true,
      base: 'main',
      staged: main ? [] : [{ path: pickF(0), status: 'M' }],
      changes: main ? [] : [
        { path: pickF(1), status: 'M' },
        { path: pickF(2), status: 'M' },
        { path: `tests/${branch.split('/').pop()}.rs`, status: '?' },
      ],
      conflicts: [],
      outgoing: main ? [] : [
        { sha: sha(`${wt}:o1`), subject: 'feat: debounce resize events' },
        { sha: sha(`${wt}:o2`), subject: 'test: cover resize storms' },
      ],
      incoming: main ? [] : [{ sha: sha(`${wt}:i1`), subject: 'docs: describe worktrees' }],
      stashes: [],
      branches: [...new Set(['main', 'next', branch])].sort(),
    };
    this.trees.set(wt, t);
    return t;
  }

  status(wt: string): ScmStatus {
    const t = this.tree(wt);
    return structuredClone({
      worktree: wt,
      project: this.projectOf(wt),
      branch: t.branch,
      head: t.head,
      upstream: t.upstream,
      base: t.base,
      ahead: t.upstream ? t.outgoing.length : 0,
      behind: t.upstream ? t.incoming.length : 0,
      staged: t.staged,
      changes: t.changes,
      conflicts: t.conflicts,
      outgoing: t.outgoing,
      incoming: t.upstream ? t.incoming : [],
      stashes: t.stashes.length,
      branches: t.branches,
      operation: null,
    });
  }

  private done(wt: string, commit?: string): ScmResult {
    const project = this.projectOf(wt);
    this.host.emit('git.changed', { project });
    this.host.emit('worktree.updated', { project });
    return commit ? { status: this.status(wt), commit } : { status: this.status(wt) };
  }

  private files(b: any): string[] | 'all' {
    if (b.files === 'all') return 'all';
    if (!Array.isArray(b.files) || !b.files.length) throw new DaemonError(400, 'files required');
    for (const f of b.files) if (!validPath(f)) throw new DaemonError(400, `invalid path ${f}`);
    return b.files;
  }

  private dirty(t: Tree): number {
    return t.staged.length + t.changes.length + t.conflicts.length;
  }

  private clean(t: Tree) {
    const n = this.dirty(t);
    if (n) throw new DaemonError(409, `${n} uncommitted change(s): commit or stash them first`);
  }

  private op_stage(wt: string, b: any): ScmResult {
    const fs = this.files(b);
    const t = this.tree(wt);
    const take = (f: ScmFile) => fs === 'all' || fs.includes(f.path);
    const moved = [...t.changes.filter(take), ...t.conflicts.filter(take)].map((f) => ({ ...f, status: f.status === '?' ? 'A' : f.status === 'U' ? 'M' : f.status }) as ScmFile);
    t.changes = t.changes.filter((f) => !take(f));
    t.conflicts = t.conflicts.filter((f) => !take(f));
    t.staged = [...t.staged.filter((f) => !moved.some((x) => x.path === f.path)), ...moved].sort((a, z) => a.path.localeCompare(z.path));
    return this.done(wt);
  }

  private op_unstage(wt: string, b: any): ScmResult {
    const fs = this.files(b);
    const t = this.tree(wt);
    const take = (f: ScmFile) => fs === 'all' || fs.includes(f.path);
    const moved = t.staged.filter(take).map((f) => ({ ...f, status: f.status === 'A' ? '?' : f.status }) as ScmFile);
    t.staged = t.staged.filter((f) => !take(f));
    t.changes = [...t.changes.filter((f) => !moved.some((x) => x.path === f.path)), ...moved].sort((a, z) => a.path.localeCompare(z.path));
    return this.done(wt);
  }

  private op_discard(wt: string, b: any): ScmResult {
    const fs = this.files(b);
    const t = this.tree(wt);
    if (fs === 'all') {
      t.staged = [];
      t.changes = [];
      t.conflicts = [];
    } else {
      t.changes = t.changes.filter((f) => !fs.includes(f.path));
      t.staged = t.staged.filter((f) => !fs.includes(f.path));
      t.conflicts = t.conflicts.filter((f) => !fs.includes(f.path));
    }
    return this.done(wt);
  }

  private op_commit(wt: string, b: any): ScmResult {
    const t = this.tree(wt);
    const message = typeof b.message === 'string' ? b.message.trim() : '';
    if (!message && !b.amend) throw new DaemonError(400, 'commit message required');
    if (b.all) this.op_stage(wt, { files: 'all' });
    if (!t.staged.length && !b.amend) throw new DaemonError(409, 'nothing to commit');
    const commit = sha(`${wt}:${t.head}:${message}:${Date.now()}`);
    t.staged = [];
    t.head = commit;
    if (b.amend && t.outgoing.length) t.outgoing[0] = { sha: commit, subject: message || t.outgoing[0].subject };
    else t.outgoing = [{ sha: commit, subject: message }, ...t.outgoing];
    if (b.push) {
      if (!t.remote) throw new DaemonError(409, `committed ${commit.slice(0, 7)} but the push failed: no remote configured`);
      t.upstream ??= `origin/${t.branch}`;
      t.outgoing = [];
    }
    return this.done(wt, commit);
  }

  private op_fetch(wt: string): ScmResult {
    const t = this.tree(wt);
    if (!t.remote) throw new DaemonError(409, 'no remote configured');
    return this.done(wt);
  }

  private op_pull(wt: string): ScmResult {
    const t = this.tree(wt);
    if (!t.branch) throw new DaemonError(409, 'HEAD is detached');
    if (!t.upstream) throw new DaemonError(409, `${t.branch} has no upstream branch`);
    t.incoming = [];
    return this.done(wt);
  }

  private op_push(wt: string): ScmResult {
    const t = this.tree(wt);
    if (!t.branch) throw new DaemonError(409, 'HEAD is detached');
    if (!t.upstream && !t.remote) throw new DaemonError(409, 'no remote configured');
    t.upstream ??= `origin/${t.branch}`;
    t.outgoing = [];
    return this.done(wt);
  }

  private op_switch(wt: string, b: any): ScmResult {
    const t = this.tree(wt);
    const name = typeof b.branch === 'string' ? b.branch : '';
    if (!BRANCH.test(name)) throw new DaemonError(400, `invalid branch name ${name}`);
    if (b.create) {
      if (t.branches.includes(name)) throw new DaemonError(409, `git switch: fatal: a branch named '${name}' already exists`);
      t.branches = [...t.branches, name].sort();
      t.upstream = null;
    } else {
      if (!t.branches.includes(name)) throw new DaemonError(409, `git switch: fatal: invalid reference: ${name}`);
      t.upstream = `origin/${name}`;
    }
    t.branch = name;
    t.outgoing = [];
    t.incoming = [];
    return this.done(wt);
  }

  private op_merge(wt: string, b: any): ScmResult {
    const t = this.tree(wt);
    const rev = typeof b.rev === 'string' ? b.rev : '';
    if (!rev || rev.startsWith('-')) throw new DaemonError(400, `invalid revision ${rev}`);
    if (!t.branches.includes(rev) && !(/^[0-9a-f]{4,64}$/i.test(rev) && this.known(wt, t, rev))) throw new DaemonError(404, `unknown revision ${rev}`);
    this.clean(t);
    const commit = sha(`${wt}:merge:${rev}`);
    t.head = commit;
    t.outgoing = [{ sha: commit, subject: `Merge ${rev} into ${t.branch ?? 'HEAD'}` }, ...t.outgoing];
    return this.done(wt);
  }

  private op_stash(wt: string, b: any): ScmResult {
    const t = this.tree(wt);
    if (b.pop) {
      const s = t.stashes.pop();
      if (!s) throw new DaemonError(409, 'no stash to pop');
      t.staged = [...t.staged, ...s.staged];
      t.changes = [...t.changes, ...s.changes];
      return this.done(wt);
    }
    if (!this.dirty(t)) throw new DaemonError(409, 'nothing to stash');
    t.stashes.push({ staged: t.staged, changes: t.changes });
    t.staged = [];
    t.changes = [];
    return this.done(wt);
  }

  private op_revert(wt: string, b: any): ScmResult {
    const t = this.tree(wt);
    const s = typeof b.sha === 'string' ? b.sha : '';
    if (!/^[0-9a-f]{4,64}$/i.test(s)) throw new DaemonError(400, 'sha must be 4-64 hex digits');
    if (!this.known(wt, t, s)) throw new DaemonError(404, `unknown commit ${s}`);
    this.clean(t);
    const commit = sha(`${wt}:revert:${s}`);
    t.head = commit;
    t.outgoing = [{ sha: commit, subject: `Revert "${s.slice(0, 7)}"` }, ...t.outgoing];
    return this.done(wt, commit);
  }
}
