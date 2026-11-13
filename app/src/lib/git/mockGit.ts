import { DaemonError } from '../daemon/client';
import type { EventType, LocalLine, Project, Task } from '../types';
import type { CleanupPlan, CommitDetail, GraphCommit, GraphPage, GraphRef, NewWorktree, Overlay, PlanItem, Worktree, WorktreeList } from './types';

export interface MockHost {
  lines: LocalLine[];
  tasks: Task[];
  projects: Project[];
  emit(type: EventType, data: unknown): void;
}

interface Repo {
  commits: GraphCommit[];
  bySha: Map<string, GraphCommit>;
  refs: GraphRef[];
  head: { sha: string; branch: string | null };
  worktrees: Worktree[];
  orphans: Worktree[];
}

const AUTHORS: [string, string][] = [
  ['Alice', 'alice@example.com'],
  ['Ada Moreau', 'ada@example.com'],
  ['Hugo Lefèvre', 'hugo@example.com'],
  ['Inès Garnier', 'ines@example.com'],
  ['Tom Rousseau', 'tom@example.com'],
  ['dependabot[bot]', 'bot@example.com'],
];
const VERBS = ['Add', 'Fix', 'Refactor', 'Simplify', 'Speed up', 'Document', 'Handle', 'Test', 'Rename', 'Drop'];
const NOUNS = [
  'session resume',
  'worktree cleanup',
  'PTY resize',
  'task ranking',
  'SSE reconnect',
  'diff parser',
  'sidebar groups',
  'agent detection',
  'config loading',
  'remote upload',
  'search index',
  'cart totals',
  'VAT rounding',
  'facet filters',
  'webhook retries',
  'error messages',
  'CI cache',
  'login redirect',
];
const FEATURES = ['search', 'billing', 'auth', 'exports', 'onboarding', 'audit-log', 'themes', 'metrics', 'imports', 'cache', 'i18n', 'retries'];
const FILES = ['src/lib.rs', 'src/daemon.rs', 'src/tasks.rs', 'app/src/App.svelte', 'app/src/lib/state.svelte.ts', 'README.md', 'tests/app.rs', 'src/api/routes.ts', 'Cargo.toml'];

function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function hashStr(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

function sha(r: () => number): string {
  let s = '';
  for (let i = 0; i < 5; i++) s += Math.floor(r() * 0xffffffff).toString(16).padStart(8, '0');
  return s;
}

export function generateHistory(name: string, size: number, agentBranches: string[], now = Date.now(), root = `/home/alice/src/${name}`) {
  const r = rng(hashStr(name));
  const pick = <T>(xs: T[]) => xs[Math.floor(r() * xs.length)];
  const commits: (GraphCommit & { t: number })[] = [];
  let t = 0;
  const add = (parents: string[], subject: string, author = pick(AUTHORS)) => {
    t += 20 * 60_000 + Math.floor(r() * 60 * 60_000);
    const c = { sha: sha(r), parents, author: author[0], email: author[1], at: new Date(t).toISOString(), subject, t };
    commits.push(c);
    return c.sha;
  };
  const subject = (prefix: string) => `${prefix}: ${pick(VERBS).toLowerCase()} ${pick(NOUNS)}`;
  let main = add([], 'Initial commit', AUTHORS[0]);
  const tags: GraphRef[] = [];
  const open = new Map<string, string>();
  let release: string | null = null;
  let featureNo = 0;
  let octopusDone = false;
  const body = size - agentBranches.length * 5 - 8;
  while (commits.length < body) {
    const x = r();
    if (x < 0.32) {
      main = add([main], subject(pick(['fix', 'chore', 'docs', 'refactor'])));
    } else if (x < 0.62 && open.size) {
      const b = pick([...open.keys()]);
      open.set(b, add([open.get(b)!], subject(b.split('/')[0] === 'fix' ? 'fix' : 'feat')));
    } else if (x < 0.74 && open.size < 5) {
      const b = `${r() < 0.7 ? 'feat' : 'fix'}/${FEATURES[featureNo++ % FEATURES.length]}${featureNo > FEATURES.length ? `-${featureNo}` : ''}`;
      open.set(b, add([main], subject('feat')));
    } else if (x < 0.86 && open.size) {
      const b = pick([...open.keys()]);
      main = add([main, open.get(b)!], `Merge branch '${b}'`);
      open.delete(b);
    } else if (!octopusDone && commits.length > body / 2) {
      const tips = ['deps/a', 'deps/b', 'deps/c'].map((d) => add([main], `chore: bump ${d.slice(5)}-lib`, AUTHORS[5]));
      main = add([main, ...tips], 'Merge branches deps/a, deps/b and deps/c');
      octopusDone = true;
    } else if (x < 0.93) {
      if (!release) release = add([main], 'release: open 2.x maintenance');
      else release = add([release], subject('fix'));
    } else if (x < 0.96) {
      main = add([main], subject('feat'));
      tags.push({ name: `v1.${tags.length}.0`, kind: 'tag', sha: main });
    } else {
      main = add([main], subject('perf'));
    }
  }
  const originMain = main;
  const tips = new Map<string, string>();
  for (let round = 0; round < 5; round++) {
    agentBranches.forEach((b, i) => {
      if (round < i % 3) return;
      tips.set(b, add([tips.get(b) ?? main], subject(round === 4 ? 'test' : 'feat')));
    });
    if (round === 1) {
      open.set('fix/sse-reconnect', add([main], 'fix: resume the event stream after a drop'));
    } else if (round === 3) {
      const b = open.has('fix/sse-reconnect') ? 'fix/sse-reconnect' : [...open.keys()][0];
      main = add([main, open.get(b)!], `Merge branch '${b}'`);
      open.delete(b);
      tags.push({ name: `v1.${tags.length}.0`, kind: 'tag', sha: main });
    } else {
      main = add([main], subject(pick(['fix', 'chore', 'docs'])));
    }
  }
  for (const [b, s] of tips) open.set(b, s);
  main = add([main], 'docs: describe worktrees', AUTHORS[0]);
  commits.sort((a, b) => b.t - a.t);
  const shift = now - 4 * 60_000 - commits[0].t;
  for (const c of commits) {
    c.t += shift;
    c.at = new Date(c.t).toISOString();
  }
  const refs: GraphRef[] = [
    { name: 'main', kind: 'branch', sha: main, upstream: 'origin/main', ahead: 2, behind: 0, worktree: root },
    { name: 'origin/main', kind: 'remote', sha: originMain },
    ...(release ? [{ name: 'release/2.x', kind: 'branch' as const, sha: release, upstream: 'origin/release/2.x', ahead: 0, behind: 0 }, { name: 'origin/release/2.x', kind: 'remote' as const, sha: release }] : []),
    ...[...open.entries()].flatMap(([b, s]) => {
      const pushed = !agentBranches.includes(b) || r() < 0.6;
      return [{ name: b, kind: 'branch' as const, sha: s, ...(pushed ? { upstream: `origin/${b}`, ahead: 0, behind: 0 } : {}) }, ...(pushed ? [{ name: `origin/${b}`, kind: 'remote' as const, sha: s }] : [])];
    }),
    ...tags,
  ];
  return { commits: commits.map(({ t: _t, ...c }) => c), refs, head: { sha: main, branch: 'main' } };
}

const DAY = 86_400_000;

export class MockGit {
  private repos = new Map<string, Repo>();

  constructor(
    private host: MockHost,
    private size = 3000,
  ) {}

  private project(raw: string): { path: string; name: string } {
    const path = decodeURIComponent(raw);
    const p = this.host.projects.find((x) => path === x.path || path.startsWith(`${x.path}/`));
    if (!p) throw new DaemonError(400, `${path} is not in a git repository`);
    return p;
  }

  private linesOf(name: string) {
    const repo = this.host.projects.find((p) => p.name === name)?.repo;
    return this.host.lines.filter((l) => l.line.repo === name || l.line.repo?.endsWith(`/${name}`) || (!!repo && l.line.repo === repo));
  }

  hasCommit(project: string, rev: string): boolean {
    const repo = this.repos.get(project);
    return !!repo && (repo.bySha.has(rev) || repo.commits.some((c) => c.sha.startsWith(rev.toLowerCase())));
  }

  projectOf(wt: string): string | undefined {
    const p = this.host.projects.find((x) => wt === x.path || wt.startsWith(`${x.path}/`));
    if (p) return p.path;
    for (const [root, repo] of this.repos) if (repo.worktrees.some((w) => w.path === wt)) return root;
    const line = this.host.lines.find((l) => l.line.worktreePath === wt);
    return line ? this.host.projects.find((x) => x.name === line.line.repo || line.line.repo?.endsWith(`/${x.name}`))?.path : undefined;
  }

  repo(path: string, name: string): Repo {
    let repo = this.repos.get(path);
    if (repo) return repo;
    const lines = this.linesOf(name);
    const g = generateHistory(name, this.size, [...new Set(lines.map((l) => l.line.branch!).filter(Boolean))], Date.now(), path);
    repo = { ...g, bySha: new Map(g.commits.map((c) => [c.sha, c])), worktrees: [], orphans: [] };
    this.repos.set(path, repo);
    this.seedWorktrees(path, name, repo);
    return repo;
  }

  store(name: string) {
    return `/home/alice/.overseer/worktrees/${name}-${hashStr(name).toString(16).padStart(8, '0').slice(0, 8)}`;
  }

  private seedWorktrees(path: string, name: string, repo: Repo) {
    const now = Date.now();
    const base = 'main';
    const mk = (w: Partial<Worktree> & Pick<Worktree, 'path' | 'branch' | 'kind' | 'state'>): Worktree => ({
      id: w.path,
      project: path,
      head: repo.refs.find((r) => r.name === w.branch)?.sha ?? repo.head.sha,
      base,
      dirty: false,
      changes: 0,
      ahead: 0,
      behind: 0,
      merged: false,
      pushed: true,
      unpushed: 0,
      lines: [],
      termIds: [],
      sizeBytes: 180_000_000,
      lastActivityAt: new Date(now - 3 * 60_000).toISOString(),
      locked: false,
      ...w,
    });
    repo.worktrees.push(mk({ path, branch: 'main', kind: 'main', state: 'idle', ahead: 2, behind: 0, sizeBytes: 412_000_000, pushed: false, unpushed: 2 }));
    const store = this.store(name);
    this.linesOf(name).forEach((l, i) => {
      const branch = l.line.branch!;
      const ref = repo.refs.find((x) => x.name === branch);
      const task = this.host.tasks.find((t) => t.lineId === l.line.id);
      const active = l.line.state === 'active';
      repo.worktrees.push(
        mk({
          path: `${store}/${branch.replace(/\//g, '-')}`,
          branch,
          kind: 'managed',
          state: active ? 'in_use' : 'idle',
          dirty: active && i % 2 === 0,
          changes: active && i % 2 === 0 ? 2 + i : 0,
          ahead: 3,
          behind: 4 + i * 3,
          pushed: !!ref?.upstream,
          unpushed: ref?.upstream ? 0 : 3,
          lines: [{ id: l.line.id, agent: l.line.agent, state: l.line.state, busy: !!l.local?.busy, needsInput: !!l.local?.needsInput }],
          termIds: this.hostTerm(l.line.id),
          task: task ? { id: task.id, number: task.number, title: task.title, status: task.status } : undefined,
          ticketKey: l.line.ticketKey ?? undefined,
          pr: l.line.prUrl ? { number: Number(l.line.prUrl.split('/').pop()), state: 'OPEN', isDraft: false, url: l.line.prUrl } : null,
          sizeBytes: 96_000_000 + i * 31_000_000,
          lastActivityAt: l.line.lastActivityAt,
        }),
      );
    });
    repo.worktrees.push(
      mk({
        path: `${store}/sb-claude-1021-1412`,
        branch: 'sb/claude/1021-1412',
        kind: 'managed',
        state: 'stale',
        merged: true,
        ahead: 0,
        behind: 37,
        pr: { number: 198, state: 'MERGED', url: `https://github.com/acme/${name}/pull/198` },
        sizeBytes: 154_000_000,
        lastActivityAt: new Date(now - 8 * DAY).toISOString(),
      }),
      mk({
        path: `${store}/task-96-csv-export`,
        branch: 'task/96-csv-export',
        kind: 'managed',
        state: 'stale',
        merged: false,
        ahead: 2,
        behind: 61,
        task: { id: 'tk_96', number: 96, title: 'CSV export', status: 'completed' },
        sizeBytes: 141_000_000,
        lastActivityAt: new Date(now - 23 * DAY).toISOString(),
      }),
      mk({ path: `${store}/sb-codex-1003-0915`, branch: 'sb/codex/1003-0915', kind: 'managed', state: 'missing', sizeBytes: null, lastActivityAt: new Date(now - 26 * DAY).toISOString() }),
    );
    repo.orphans.push(
      mk({ path: `${path}-hotfix`, branch: 'hotfix/login-loop', kind: 'orphan', state: 'idle', ahead: 1, behind: 12, pushed: false, unpushed: 1, sizeBytes: 388_000_000, lastActivityAt: new Date(now - 5 * DAY).toISOString() }),
    );
  }

  private hostTerm(lineId: string): string[] {
    const host = this.host as unknown as { terms?: { id: string; lineId?: string | null; exited: boolean }[] };
    return (host.terms ?? []).filter((t) => t.lineId === lineId && !t.exited).map((t) => t.id);
  }

  route(method: string, path: string, query: URLSearchParams, body: any): unknown {
    let m: RegExpMatchArray | null;
    if ((m = path.match(/^\/v1\/projects\/([^/]+)\/graph$/)) && method === 'GET') return this.graph(m[1], query);
    if ((m = path.match(/^\/v1\/projects\/([^/]+)\/commits\/([^/]+)$/)) && method === 'GET') return this.commit(m[1], decodeURIComponent(m[2]));
    if ((m = path.match(/^\/v1\/projects\/([^/]+)\/branches$/)) && method === 'POST') return this.branch(m[1], body);
    if ((m = path.match(/^\/v1\/projects\/([^/]+)\/worktrees$/)) && method === 'GET') return this.list(m[1]);
    if (path === '/v1/worktrees' && method === 'POST') return this.create(body);
    if ((m = path.match(/^\/v1\/worktrees\/([^/]+)\/(cleanup|merge-base|cherry-pick)$/)) && method === 'POST') return this.action(decodeURIComponent(m[1]), m[2], body);
    return undefined;
  }

  private overlays(p: { path: string; name: string }, repo: Repo): Overlay[] {
    const out: Overlay[] = [];
    for (const w of repo.worktrees) {
      if (w.kind !== 'managed' || !w.branch) continue;
      const ref = repo.refs.find((r) => r.kind === 'branch' && r.name === w.branch);
      if (!ref) continue;
      const l = w.lines[0];
      out.push({
        branch: w.branch,
        sha: ref.sha,
        worktree: w.path,
        ...(l ? { lineId: l.id, agent: l.agent, state: l.state, busy: l.busy, needsInput: l.needsInput } : {}),
        ...(w.task ? { taskId: w.task.id, taskNumber: w.task.number, taskStatus: w.task.status } : {}),
        ...(w.ticketKey ? { ticketKey: w.ticketKey } : {}),
        ...(w.pr ? { pr: w.pr } : {}),
      });
    }
    return out;
  }

  private reachable(repo: Repo, from: string[]): Set<string> {
    const seen = new Set<string>();
    const stack = from.slice();
    while (stack.length) {
      const s = stack.pop()!;
      if (seen.has(s)) continue;
      seen.add(s);
      for (const p of repo.bySha.get(s)?.parents ?? []) stack.push(p);
    }
    return seen;
  }

  graph(raw: string, q: URLSearchParams): GraphPage {
    const p = this.project(raw);
    const repo = this.repo(p.path, p.name);
    const limit = Math.min(2000, Math.max(1, Number(q.get('limit') ?? 300) || 300));
    const offset = Number(q.get('before') ?? 0) || 0;
    const refs = q.get('refs') ?? 'all';
    const overlays = this.overlays(p, repo);
    let list = repo.commits;
    if (refs !== 'all') {
      const tips =
        refs === 'current'
          ? [repo.head.sha]
          : refs === 'agents'
            ? [repo.head.sha, ...overlays.map((o) => o.sha)]
            : refs.split(',').flatMap((n) => repo.refs.filter((r) => r.name === n).map((r) => r.sha));
      const keep = this.reachable(repo, tips);
      list = list.filter((c) => keep.has(c.sha));
    }
    const commits = list.slice(offset, offset + limit);
    const next = offset + limit < list.length ? String(offset + limit) : undefined;
    return {
      project: p.path,
      repo: this.host.projects.find((x) => x.path === p.path)?.repo,
      base: 'main',
      head: { ...repo.head },
      refs: structuredClone(repo.refs),
      overlays,
      commits: structuredClone(commits),
      ...(next ? { next } : {}),
    };
  }

  commit(raw: string, sha: string): CommitDetail {
    if (!/^[0-9a-f]{4,64}$/i.test(sha)) throw new DaemonError(400, 'sha must be 4-64 hex digits');
    const p = this.project(raw);
    const repo = this.repo(p.path, p.name);
    const c = repo.commits.find((x) => x.sha.startsWith(sha.toLowerCase()));
    if (!c) throw new DaemonError(404, `unknown commit ${sha}`);
    const r = rng(hashStr(c.sha));
    const n = c.parents.length > 1 ? 0 : 1 + Math.floor(r() * 4);
    const files = Array.from({ length: n }, (_, i) => {
      const path = FILES[(hashStr(c.sha) + i * 3) % FILES.length];
      const added = 1 + Math.floor(r() * 18);
      const removed = Math.floor(r() * 9);
      return { path, status: i === 0 && r() < 0.2 ? 'A' : 'M', added, removed, binary: false };
    });
    const unique = files.filter((f, i) => files.findIndex((g) => g.path === f.path) === i);
    const diff = unique.map((f) => fakeDiff(f.path, f.added, f.removed, c.subject)).join('');
    return {
      ...structuredClone(c),
      committer: c.author,
      committedAt: c.at,
      body: c.parents.length > 1 ? '' : `${c.subject.replace(/^\w+: /, '').replace(/^./, (x) => x.toUpperCase())}.\n\nKeeps behaviour unchanged for existing callers.`,
      refs: repo.refs.filter((x) => x.sha === c.sha).map((x) => x.name),
      files: unique,
      diff,
      truncated: false,
    };
  }

  private branch(raw: string, body: any) {
    const p = this.project(raw);
    const repo = this.repo(p.path, p.name);
    const name = String(body?.name ?? '').trim();
    if (!name || /\s|\.\.|^[-/]|\/$/.test(name)) throw new DaemonError(400, `'${name}' is not a valid branch name`);
    if (!repo.bySha.has(body?.sha)) throw new DaemonError(400, `unknown commit ${body?.sha}`);
    if (repo.refs.some((r) => r.kind === 'branch' && r.name === name)) throw new DaemonError(409, `branch ${name} exists`);
    repo.refs.push({ name, kind: 'branch', sha: body.sha });
    this.host.emit('git.changed', { project: p.path });
    return { name, sha: body.sha };
  }

  list(raw: string): WorktreeList {
    const p = this.project(raw);
    const repo = this.repo(p.path, p.name);
    return { project: p.path, store: this.store(p.name), base: 'main', worktrees: structuredClone(repo.worktrees), orphans: structuredClone(repo.orphans) };
  }

  createIn(project: string, opts: Omit<NewWorktree, 'project'>): Worktree {
    return this.create({ ...opts, project });
  }

  private create(body: NewWorktree): Worktree {
    const p = this.project(encodeURIComponent(body?.project ?? ''));
    const repo = this.repo(p.path, p.name);
    const baseSha = body.base ? repo.commits.find((c) => c.sha.startsWith(body.base!))?.sha ?? repo.refs.find((r) => r.name === body.base)?.sha : repo.head.sha;
    if (!baseSha) throw new DaemonError(400, `unknown base ${body.base}`);
    const slug = body.slug ?? (body.agent ? stamp() : baseSha.slice(0, 7));
    let branch = body.branch ?? (body.agent ? `sb/${body.agent}/${slug}` : `sb/${slug}`);
    if (repo.worktrees.some((w) => w.branch === branch)) {
      if (body.branch) throw new DaemonError(409, `${branch} is already checked out`);
      let n = 2;
      while (repo.worktrees.some((w) => w.branch === `${branch}-${n}`)) n++;
      branch = `${branch}-${n}`;
    }
    const w: Worktree = {
      id: `${this.store(p.name)}/${branch.replace(/\//g, '-')}`,
      path: `${this.store(p.name)}/${branch.replace(/\//g, '-')}`,
      project: p.path,
      branch,
      head: baseSha,
      kind: 'managed',
      base: 'main',
      state: 'idle',
      dirty: false,
      changes: 0,
      ahead: 0,
      behind: 0,
      merged: false,
      pushed: false,
      unpushed: 0,
      lines: [],
      termIds: [],
      sizeBytes: null,
      lastActivityAt: new Date().toISOString(),
      locked: false,
    };
    w.id = w.path;
    repo.worktrees.push(w);
    if (!repo.refs.some((r) => r.name === branch)) repo.refs.push({ name: branch, kind: 'branch', sha: baseSha, worktree: w.path });
    this.host.emit('worktree.created', { worktree: w });
    this.host.emit('git.changed', { project: p.path });
    return structuredClone(w);
  }

  private find(id: string): { repo: Repo; w: Worktree; orphan: boolean } {
    for (const repo of this.repos.values()) {
      const w = repo.worktrees.find((x) => x.id === id);
      if (w) return { repo, w, orphan: false };
      const o = repo.orphans.find((x) => x.id === id);
      if (o) return { repo, w: o, orphan: true };
    }
    throw new DaemonError(404, `${id} is not a worktree of a repository`);
  }

  plan(w: Worktree, dryRun: boolean): CleanupPlan {
    const refuse = (refused: string): CleanupPlan => ({ dryRun, id: w.id, refused, remove: [], keep: [] });
    if (w.kind === 'main') return refuse('this is the main checkout');
    if (w.kind === 'orphan') return refuse('created outside Overseer: not touched');
    if (w.state === 'missing') return { dryRun, id: w.id, refused: null, remove: [{ kind: 'prune', path: w.path }], keep: [] };
    if (w.locked) return refuse('the worktree is locked');
    if (w.state === 'in_use') return refuse(w.lines.length ? `in use by ${w.lines[0].agent} (${w.lines[0].id})` : 'in use');
    if (w.dirty) return refuse(`${w.changes} uncommitted change${w.changes === 1 ? '' : 's'}`);
    if (w.unpushed > 0) return refuse(`${w.unpushed} commit${w.unpushed === 1 ? '' : 's'} not pushed or merged`);
    const remove: PlanItem[] = [{ kind: 'worktree', path: w.path }];
    const keep: PlanItem[] = [];
    if (w.branch && w.merged) remove.push({ kind: 'branch', name: w.branch });
    else if (w.branch) keep.push({ kind: 'branch', name: w.branch, reason: `not merged into ${w.base}` });
    return { dryRun, id: w.id, refused: null, remove, keep };
  }

  private action(id: string, kind: string, body: any): unknown {
    const { repo, w } = this.find(id);
    if (kind === 'cleanup') {
      const dry = body?.dryRun !== false;
      const plan = this.plan(w, dry);
      if (dry) return plan;
      if (plan.refused) throw new DaemonError(409, plan.refused);
      repo.worktrees = repo.worktrees.filter((x) => x.id !== id);
      if (plan.remove.some((x) => x.kind === 'branch')) repo.refs = repo.refs.filter((r) => !(r.kind === 'branch' && r.name === w.branch));
      this.host.emit('worktree.removed', { project: w.project, id });
      this.host.emit('git.changed', { project: w.project });
      return plan;
    }
    if (w.dirty) throw new DaemonError(409, `${w.changes} uncommitted changes in ${w.path}`);
    if (kind === 'merge-base') {
      w.behind = 0;
      w.ahead = (w.ahead ?? 0) + 1;
      w.lastActivityAt = new Date().toISOString();
      this.host.emit('worktree.updated', { project: w.project });
      return structuredClone(w);
    }
    const s = String(body?.sha ?? '');
    if (!/^[0-9a-f]{4,64}$/i.test(s)) throw new DaemonError(400, 'sha must be 4-64 hex digits');
    w.ahead = (w.ahead ?? 0) + 1;
    w.unpushed += 1;
    w.lastActivityAt = new Date().toISOString();
    this.host.emit('worktree.updated', { project: w.project });
    return { commit: sha(rng(hashStr(s + w.id))), worktree: structuredClone(w) };
  }
}

function stamp(d = new Date()) {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getUTCMonth() + 1)}${p(d.getUTCDate())}-${p(d.getUTCHours())}${p(d.getUTCMinutes())}`;
}

function fakeDiff(path: string, added: number, removed: number, subject: string): string {
  const lines: string[] = [`diff --git a/${path} b/${path}`, `--- a/${path}`, `+++ b/${path}`, `@@ -10,${removed + 3} +10,${added + 3} @@`];
  lines.push(' fn handle(req: &Request) -> Response {');
  for (let i = 0; i < removed; i++) lines.push(`-    let old_${i} = legacy(req, ${i});`);
  for (let i = 0; i < added; i++) lines.push(i === 0 ? `+    // ${subject}` : `+    let v${i} = step(req, ${i})?;`);
  lines.push('     respond(req)', ' }');
  return lines.join('\n') + '\n';
}
