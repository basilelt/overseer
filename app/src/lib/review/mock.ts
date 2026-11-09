import { DaemonError } from '../client';
import type { EventType, LocalLine, Task } from '../types';
import { lineDiff } from './linediff';
import { annotate, mockGraph } from './mocktriage';
import { mockMap } from './mockmap';
import { threadSuggestion } from './threads';
import type { Bases, Branches, Delivery, Related, ReviewCommit, ReviewComment, ReviewDiff, ReviewFile, Side, SubmitKind, Thread } from './types';

export const MOCK_WORKTREE = '/home/alice/worktrees/overseer/feat-term-resize';
export const MOCK_ROOT = '/home/alice/Documents/Github/work/overseer';
export const MOCK_MISSING = '/home/alice/Documents/Github/work/cmux-ide';
export const MOCK_SUBMODULE = `${MOCK_WORKTREE}/vendor/ghostty-web`;
const MOCK_WORKSPACES = '/home/alice/worktrees/overseer/feat-workspaces';
const MOCK_AGENT_WT = `${MOCK_ROOT}/.claude/worktrees/review-fix`;
export const MOCK_LINKED = '/home/alice/Documents/Github/work/overseer-mobile-wt';
const LINE = 'ln_03';
const TASK = 'tk_10';
const TERM = 'tm_ln_03';

const TERM_RS_BASE = `use crate::board::publish;
use portable_pty::{MasterPty, PtySize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Term {
    id: String,
    master: Mutex<Box<dyn MasterPty + Send>>,
    cols: u16,
    rows: u16,
}

impl Term {
    pub fn new(id: String, master: Box<dyn MasterPty + Send>) -> Term {
        Term {
            id,
            master: Mutex::new(master),
            cols: 80,
            rows: 24,
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), String> {
        self.master
            .lock()
            .unwrap()
            .resize(PtySize { rows, cols, ..Default::default() })
            .map_err(|e| e.to_string())?;
        self.cols = cols;
        self.rows = rows;
        publish("term.updated", json!({"id": self.id, "cols": cols, "rows": rows}));
        Ok(())
    }

    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

pub fn spawn_all(terms: &[Arc<Term>]) {
    for t in terms {
        let _ = t.id();
    }
}

pub fn wait(d: Duration) {
    std::thread::sleep(d);
}
`;

const TERM_RS_NEW = `use crate::board::publish;
use crate::resize::Debounce;
use portable_pty::{MasterPty, PtySize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RESIZE_QUIET: Duration = Duration::from_millis(50);

pub struct Term {
    id: String,
    master: Mutex<Box<dyn MasterPty + Send>>,
    cols: u16,
    rows: u16,
    pending: Debounce<(u16, u16)>,
}

impl Term {
    pub fn new(id: String, master: Box<dyn MasterPty + Send>) -> Term {
        Term {
            id,
            master: Mutex::new(master),
            cols: 80,
            rows: 24,
            pending: Debounce::new(RESIZE_QUIET),
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.pending.push((cols, rows), Instant::now());
    }

    pub fn flush_resize(&mut self, now: Instant) -> Result<bool, String> {
        let Some((cols, rows)) = self.pending.take_ready(now) else {
            return Ok(false);
        };
        if (cols, rows) == (self.cols, self.rows) {
            return Ok(false);
        }
        self.master
            .lock()
            .unwrap()
            .resize(PtySize { rows, cols, ..Default::default() })
            .map_err(|e| e.to_string())?;
        self.cols = cols;
        self.rows = rows;
        publish("term.updated", json!({"id": self.id, "cols": cols, "rows": rows}));
        Ok(true)
    }

    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

pub fn spawn_all(terms: &[Arc<Term>]) {
    for t in terms {
        let _ = t.id();
    }
}

pub fn wait(d: Duration) {
    std::thread::sleep(d);
}
`;

const RESIZE_BASE = `use std::time::{Duration, Instant};

pub struct Debounce<T> {
    quiet: Duration,
    value: Option<(T, Instant)>,
}

impl<T> Debounce<T> {
    pub fn new(quiet: Duration) -> Self {
        Debounce { quiet, value: None }
    }

    pub fn push(&mut self, v: T, at: Instant) {
        self.value = Some((v, at));
    }

    pub fn take(&mut self, now: Instant) -> Option<T> {
        match &self.value {
            Some((_, at)) if now.duration_since(*at) >= self.quiet => self.value.take().map(|(v, _)| v),
            _ => None,
        }
    }
}
`;

const RESIZE_NEW = RESIZE_BASE.replace('pub fn take(&mut self', 'pub fn take_ready(&mut self').replace(
  '    pub fn push(&mut self, v: T, at: Instant) {\n        self.value = Some((v, at));\n    }\n',
  '    pub fn push(&mut self, v: T, at: Instant) {\n        self.value = Some((v, at));\n    }\n\n    pub fn is_pending(&self) -> bool {\n        self.value.is_some()\n    }\n',
);

const TERMINAL_TS_BASE = `import type { TermIO } from './client';

export interface Size {
  cols: number;
  rows: number;
}

export function fit(el: HTMLElement, cell: Size): Size {
  const r = el.getBoundingClientRect();
  return { cols: Math.floor(r.width / cell.cols), rows: Math.floor(r.height / cell.rows) };
}

export function watchSize(el: HTMLElement, cell: Size, io: TermIO): () => void {
  const ro = new ResizeObserver(() => {
    const s = fit(el, cell);
    io.resize(s.cols, s.rows);
  });
  ro.observe(el);
  return () => ro.disconnect();
}
`;

const TERMINAL_TS_NEW = `import type { TermIO } from './client';

export interface Size {
  cols: number;
  rows: number;
}

export function fit(el: HTMLElement, cell: Size): Size {
  const r = el.getBoundingClientRect();
  return { cols: Math.max(2, Math.floor(r.width / cell.cols)), rows: Math.max(1, Math.floor(r.height / cell.rows)) };
}

export function watchSize(el: HTMLElement, cell: Size, io: TermIO, delay = 50): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let last = '';
  const ro = new ResizeObserver(() => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      const s = fit(el, cell);
      const key = \`\${s.cols}x\${s.rows}\`;
      if (key === last) return;
      last = key;
      io.resize(s.cols, s.rows);
    }, delay);
  });
  ro.observe(el);
  return () => {
    clearTimeout(timer);
    ro.disconnect();
  };
}
`;

const TEST_NEW = `use overseer::resize::Debounce;
use std::time::{Duration, Instant};

#[test]
fn keeps_the_last_size_only() {
    let mut d = Debounce::new(Duration::from_millis(50));
    let t0 = Instant::now();
    d.push((80, 24), t0);
    d.push((120, 32), t0 + Duration::from_millis(10));
    assert_eq!(d.take_ready(t0 + Duration::from_millis(20)), None);
    assert_eq!(d.take_ready(t0 + Duration::from_millis(70)), Some((120, 32)));
    assert!(!d.is_pending());
}

#[test]
fn nothing_pending_at_start() {
    let mut d: Debounce<(u16, u16)> = Debounce::new(Duration::from_millis(50));
    assert_eq!(d.take_ready(Instant::now()), None);
}
`;

const LEGACY_BASE = `use std::io::Write;

pub fn resize_now(fd: i32, cols: u16, rows: u16) -> std::io::Result<()> {
    let ws = libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
    if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &ws) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    std::io::stdout().flush()
}
`;

interface MockFile {
  path: string;
  oldPath: string;
  status: ReviewFile['status'];
  base: string | null;
  now: string | null;
}

const lines = (s: string | null) => (s === null ? [] : s.replace(/\n$/, '').split('\n'));

function hash(s: string): string {
  let h = 0xcbf29ce484222325n;
  for (const c of new TextEncoder().encode(s)) h = BigInt.asUintN(64, (h ^ BigInt(c)) * 0x100000001b3n);
  return h.toString(16).padStart(16, '0');
}

export const bigDiff = () => typeof location !== 'undefined' && new URLSearchParams(location.search).has('bigdiff');

export function bigFiles(n = 50, size = 400): MockFile[] {
  const out: MockFile[] = [];
  for (let f = 0; f < n; f++) {
    const base: string[] = [];
    const now: string[] = [];
    for (let i = 0; i < size; i++) {
      const line = i % 23 === 0 ? `  const record${i} = { id: "${'x'.repeat(40)}${i}", values: [${Array.from({ length: 30 }, (_, k) => k * i).join(', ')}] }; // long line ${i}` : `  export function handler${f}_${i}(value: number): number { return value * ${i} + ${f}; }`;
      base.push(line);
      if (i % 9 === 0) now.push(line.replace('return value', 'return Math.max(0, value)'));
      else if (i % 13 !== 0) now.push(line);
      if (i % 17 === 0) now.push(`  const added${i} = "inserted line ${i} in file ${f}";`);
    }
    const path = `src/gen/module${String(f).padStart(2, '0')}.ts`;
    out.push({ path, oldPath: path, status: 'modified', base: base.join('\n') + '\n', now: now.join('\n') + '\n' });
  }
  return out;
}

let patch = '';
export function bigPatch(): string {
  patch ||= bigFiles()
    .map((f) => [`diff --git a/${f.path} b/${f.path}`, `--- a/${f.path}`, `+++ b/${f.path}`, ...lineDiff(lines(f.base), lines(f.now)).flatMap((h) => [h.header, ...h.lines.map((l) => (l.kind === 'add' ? '+' : l.kind === 'del' ? '-' : ' ') + l.text)])].join('\n'))
    .join('\n');
  return new URLSearchParams(location.search).get('bigdiff') === 'live' ? patch.replace('in file 0', `in file 0 at ${Date.now()}`) : patch;
}

const STRINGS_BASE = `use std::borrow::Cow;

pub fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        let space = ch.is_whitespace();
        if !(space && prev_space) {
            out.push(if space { ' ' } else { ch });
        }
        prev_space = space;
    }
    out
}

pub fn shorten(s: &str, max: usize) -> Cow<'_, str> {
    if s.len() <= max {
        return Cow::Borrowed(s);
    }
    let cut = s.char_indices().nth(max.saturating_sub(1)).map(|(i, _)| i).unwrap_or(s.len());
    Cow::Owned(format!("{}…", &s[..cut]))
}

pub fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

pub const SEPARATORS: [char; 3] = [' ',',',';'];
`;
const STRINGS_NEW = STRINGS_BASE.replace(/pub fn collapse_spaces[\s\S]*?\n}\n\n/, '').replace("[' ',',',';']", "[' ', ',', ';']");
const TEXT_NEW = `pub fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        let space = ch.is_whitespace();
        if !(space && prev_space) {
            out.push(if space { ' ' } else { ch });
        }
        prev_space = space;
    }
    out
}

pub fn title(s: &str) -> String {
    collapse_spaces(s).trim().to_string()
}
`;

const iso = (minsAgo: number) => new Date(Date.now() - minsAgo * 60_000).toISOString();

type Emit = (type: EventType, data: unknown) => void;
type Data = () => { lines: LocalLine[]; tasks: Task[] };

export class MockReview {
  files: MockFile[] = [
    { path: 'app/src/lib/terminal.ts', oldPath: 'app/src/lib/terminal.ts', status: 'modified', base: TERMINAL_TS_BASE, now: TERMINAL_TS_NEW },
    { path: 'src/pty_legacy.rs', oldPath: 'src/pty_legacy.rs', status: 'deleted', base: LEGACY_BASE, now: null },
    { path: 'src/resize.rs', oldPath: 'src/debounce.rs', status: 'renamed', base: RESIZE_BASE, now: RESIZE_NEW },
    { path: 'src/term.rs', oldPath: 'src/term.rs', status: 'modified', base: TERM_RS_BASE, now: TERM_RS_NEW },
    { path: 'tests/resize.rs', oldPath: 'tests/resize.rs', status: 'untracked', base: null, now: TEST_NEW },
    { path: 'src/util/strings.rs', oldPath: 'src/util/strings.rs', status: 'modified', base: STRINGS_BASE, now: STRINGS_NEW },
    { path: 'src/text.rs', oldPath: 'src/text.rs', status: 'untracked', base: null, now: TEXT_NEW },
  ];
  viewed: Record<string, string> = {};
  threads: Thread[];
  private seq = 0;
  private timers: ReturnType<typeof setTimeout>[] = [];

  constructor(
    private emit: Emit,
    private data: Data,
  ) {
    this.threads = this.seed();
    if (bigDiff()) this.files.push(...bigFiles());
    this.viewed['src/pty_legacy.rs'] = this.fileJson(this.files[1]).hash;
  }

  private id(prefix: string) {
    return `${prefix}-mock${String(++this.seq).padStart(4, '0')}`;
  }

  private comment(author: 'user' | 'agent', body: string, ago: number, pending = false): ReviewComment {
    return { id: this.id('rc'), author, body, pending, createdAt: iso(ago), updatedAt: iso(ago) };
  }

  private delivery(status: Delivery['status'], ago: number, threads: string[], kind: SubmitKind = 'request_changes'): Delivery {
    return { id: this.id('rd'), status, kind, target: status === 'none' ? null : `term:${TERM}`, reason: status === 'queued' ? 'the agent is busy' : null, at: iso(ago), threads };
  }

  private thread(path: string, line: number, endLine: number, status: Thread['status'], comments: ReviewComment[], extra: Partial<Thread> = {}): Thread {
    const code = lines(this.files.find((f) => f.path === path)?.now ?? '')
      .slice(line - 1, endLine)
      .join('\n');
    const created = comments[0]?.createdAt ?? iso(5);
    return {
      id: this.id('rt'),
      worktree: MOCK_WORKTREE,
      path,
      side: 'new',
      line,
      endLine,
      originalLine: line,
      originalEndLine: endLine,
      code,
      status,
      outdated: false,
      changed: false,
      lineId: LINE,
      taskId: TASK,
      createdAt: created,
      updatedAt: created,
      comments,
      ...extra,
    };
  }

  private seed(): Thread[] {
    const t: Thread[] = [];
    const delivered = this.delivery('delivered', 18, []);
    const queued = this.delivery('queued', 1, []);
    t.push(
      this.thread('src/term.rs', 8, 8, 'sent', [this.comment('user', 'Make this configurable from `config.json` (`resizeQuietMs`), 50 ms is too eager over SSH.', 2)], { delivery: queued }),
    );
    t.push(
      this.thread('src/term.rs', 37, 39, 'sent', [
        this.comment('user', 'Return early before taking the lock, and skip the publish when nothing changed:\n\n```suggestion\n        if (cols, rows) == (self.cols, self.rows) {\n            return Ok(false);\n        }\n```', 18),
      ], { delivery: delivered, changed: true, code: '        self.master\n            .lock()\n            .unwrap()' }),
    );
    t.push(
      this.thread('src/term.rs', 29, 31, 'addressed', [
        this.comment('user', '`resize` no longer reports errors. Who surfaces a failed ioctl now?', 25),
        this.comment('agent', 'flush_resize returns the error; the tick in daemon.rs logs it and publishes term.updated only on success. Added a test.', 12),
      ], { delivery: delivered }),
    );
    t.push(this.thread('src/term.rs', 15, 15, 'pending', [this.comment('user', 'Nit: `pending` reads like a bool. `next_size`?', 0, true)]));
    t.push(
      this.thread('app/src/lib/terminal.ts', 10, 10, 'sent', [
        this.comment('user', 'Hard to read on one line:\n\n```suggestion\n  const cols = Math.floor(r.width / cell.cols);\n  const rows = Math.floor(r.height / cell.rows);\n  return { cols: Math.max(2, cols), rows: Math.max(1, rows) };\n```', 16),
      ], { delivery: delivered }),
    );
    t.push(
      this.thread('app/src/lib/terminal.ts', 17, 17, 'sent', [this.comment('user', 'Use requestAnimationFrame instead of a fixed timeout?', 40)], {
        outdated: true,
        line: 17,
        endLine: 17,
        originalLine: 17,
        originalEndLine: 17,
        code: '    requestAnimationFrame(() => {',
        delivery: delivered,
      }),
    );
    t.push(
      this.thread('tests/resize.rs', 9, 9, 'resolved', [
        this.comment('user', 'Also assert that a push after take_ready starts a new window.', 50),
        this.comment('agent', 'Done in keeps_the_last_size_only.', 44),
      ], { delivery: delivered }),
    );
    delivered.threads = t.filter((x) => x.delivery?.id === delivered.id).map((x) => x.id);
    queued.threads = [t[0].id];
    return t;
  }

  worktreeOf(q: URLSearchParams | Record<string, unknown>): string {
    const get = (k: string) => (q instanceof URLSearchParams ? q.get(k) : typeof q[k] === 'string' ? (q[k] as string) : null);
    const wt = get('worktree');
    if (wt) return wt;
    const { lines: ls, tasks } = this.data();
    const lineId = get('line') ?? get('lineId');
    const taskId = get('task') ?? get('taskId');
    if (lineId) {
      if (lineId === LINE) return MOCK_WORKTREE;
      const l = ls.find((x) => x.line.id === lineId);
      if (!l) throw new DaemonError(404, `no line ${lineId}`);
      if (l.line.repo === 'unknown') throw new DaemonError(400, `${l.line.worktreePath} is not in a git worktree`);
      return l.line.worktreePath ?? '';
    }
    if (taskId) {
      const task = tasks.find((x) => x.id === taskId);
      if (!task) throw new DaemonError(404, `no task ${taskId}`);
      if (task.lineId === LINE || task.id === TASK) return MOCK_WORKTREE;
      return task.worktreePath ?? ls.find((x) => x.line.id === task.lineId)?.line.worktreePath ?? '';
    }
    throw new DaemonError(400, 'worktree, line or task required');
  }

  private hunks = new Map<string, ReviewFile['hunks']>();

  private fileJson(f: MockFile): ReviewFile {
    const key = `${f.base}\0${f.now}`;
    const cached = this.hunks.get(key);
    const hunks = cached ? structuredClone(cached) : lineDiff(lines(f.base), lines(f.now));
    if (!cached && bigDiff()) this.hunks.set(key, structuredClone(hunks));
    const add = hunks.reduce((n, h) => n + h.lines.filter((l) => l.kind === 'add').length, 0);
    const del = hunks.reduce((n, h) => n + h.lines.filter((l) => l.kind === 'del').length, 0);
    const h = hash(JSON.stringify(hunks) + f.status);
    return { path: f.path, oldPath: f.oldPath, status: f.status, binary: false, additions: add, deletions: del, hash: h, viewed: this.viewed[f.path] === h, truncated: false, hunks };
  }

  private side: Record<string, { branch: string; files: MockFile[]; commits: string[]; ago: number; agents: number }> = {
    [MOCK_WORKSPACES]: {
      branch: 'feat/workspaces',
      ago: 2,
      agents: 1,
      commits: ['Group sessions by workspace in the sidebar'],
      files: [
        { path: 'app/src/lib/workspaces.ts', oldPath: 'app/src/lib/workspaces.ts', status: 'modified', base: 'export const MAX = 8;\nexport function name(p: string) {\n  return p;\n}\n', now: 'export const MAX = 16;\nexport function name(p: string) {\n  return p.split(\'/\').pop() ?? p;\n}\n' },
        { path: 'app/src/lib/workspaces.test.ts', oldPath: 'app/src/lib/workspaces.test.ts', status: 'untracked', base: null, now: "import { name } from './workspaces';\n\nname('/a/b');\n" },
      ],
    },
    [MOCK_AGENT_WT]: {
      branch: 'fix/review-panel',
      ago: 6,
      agents: 0,
      commits: ['fix(review): base the default branch on its upstream', 'feat(review): list related worktrees'],
      files: [{ path: 'src/review/diff.rs', oldPath: 'src/review/diff.rs', status: 'modified', base: 'pub fn base() -> Option<String> {\n    None\n}\n', now: 'pub fn base() -> Option<String> {\n    upstream()\n}\n' }],
    },
  };

  private linkedSide: Record<string, { branch: string; files: MockFile[]; commits: string[]; ago: number; agents: number }> = {
    [MOCK_LINKED]: {
      branch: 'feat/term-resize',
      ago: 4,
      agents: 0,
      commits: ['Resize the terminal on rotation'],
      files: [
        { path: 'android/app/src/main/Terminal.kt', oldPath: 'android/app/src/main/Terminal.kt', status: 'modified', base: 'fun resize(cols: Int) {\n    pty.cols = cols\n}\n', now: 'fun resize(cols: Int, rows: Int) {\n    pty.cols = cols\n    pty.rows = rows\n}\n' },
        { path: 'android/app/src/main/Rotation.kt', oldPath: 'android/app/src/main/Rotation.kt', status: 'untracked', base: null, now: 'fun onRotate() = resize(cols(), rows())\n' },
      ],
    },
  };

  private commit(i: number, subject: string, ago: number): ReviewCommit {
    return { sha: hash(subject + i).padEnd(40, '0').slice(0, 40), subject, author: i % 3 ? 'Alice' : 'Claude', at: iso(ago) };
  }

  recentOnMain(): ReviewCommit[] {
    return [
      'Merge pull request #18 from acme/feat/hooks-all-agents',
      'fix(hooks): fall back to terminal activity when a turn-start hook is missing',
      'Merge pull request #17 from acme/feat/crash-logs',
      'fix(app): panic hook parameter shadowed version()',
      'style: rustfmt dlog',
      'Merge pull request #16 from acme/fix/other-sessions',
      'feat(tasks): turn agent sessions into Kanban tasks that follow them',
      'docs: CI budget and release workflow',
    ].map((m, i) => this.commit(i, m, 25 + i * 70));
  }

  related(current: string): Related {
    const known = [MOCK_ROOT, MOCK_WORKTREE, MOCK_WORKSPACES, MOCK_AGENT_WT];
    if (!known.includes(current)) return { current, worktrees: [{ path: current, branch: null, files: 0, dirty: 0, ahead: 0, agents: 0, current: true, main: true, at: iso(60) }] };
    const term = this.diff(MOCK_WORKTREE);
    const w = [
      { path: MOCK_WORKTREE, branch: 'feat/term-resize', files: term.files.length, dirty: 2, ahead: 3, agents: 1, at: iso(1) },
      ...Object.entries(this.side).map(([path, x]) => ({ path, branch: x.branch, files: x.files.length, dirty: x.files.length, ahead: x.commits.length, agents: x.agents, at: iso(x.ago) })),
      { path: MOCK_ROOT, branch: 'main', files: 0, dirty: 0, ahead: 0, agents: 1, at: iso(25) },
    ];
    return {
      current,
      worktrees: w
        .map((x) => ({ ...x, base: x.branch === 'main' ? 'origin/main' : 'main', head: null, current: x.path === current, main: x.path === MOCK_ROOT }))
        .filter((x) => x.current || (x.agents && (x.files || x.ahead))),
    };
  }

  branches(): Branches {
    return {
      base: 'main',
      branches: [
        { name: 'feat/term-resize', head: 'a1', at: iso(1), worktree: MOCK_WORKTREE, subject: 'Debounce PTY resizes' },
        { name: 'fix/ssh-reconnect', head: 'b2', at: iso(180), worktree: null, subject: 'Reconnect the SSH tunnel after sleep' },
        { name: 'docs/ci-budget', head: 'c3', at: iso(1500), worktree: null, subject: 'Document the CI budget' },
      ],
    };
  }

  bases(wt: string): Bases {
    const cur = this.diff(wt).branch;
    const b = (name: string, remote: boolean, ago: number, subject: string) => ({ name, remote, head: hash(name).slice(0, 12), at: iso(ago), subject });
    return {
      base: 'main',
      baseSource: 'created',
      default: 'main',
      branches: [
        b('main', false, 25, 'Merge pull request #18 from acme/feat/hooks-all-agents'),
        b('feat/workspaces', false, 40, 'Group sessions by workspace in the sidebar'),
        b('release/next', false, 90, 'Merge branch feat/notification-center'),
        b('fix/ssh-reconnect', false, 180, 'Reconnect the SSH tunnel after sleep'),
        b('origin/main', true, 25, 'Merge pull request #18 from acme/feat/hooks-all-agents'),
        b('origin/release/next', true, 95, 'Merge branch feat/notification-center'),
        b('origin/feat/term-resize', true, 300, 'Debounce PTY resizes'),
      ].filter((x) => x.name !== cur),
    };
  }

  private branchDiff(wt: string, branch: string): ReviewDiff {
    const files = [
      this.fileJson({ path: 'src/remote.rs', oldPath: 'src/remote.rs', status: 'modified', base: 'fn connect() {\n    dial();\n}\n', now: 'fn connect() {\n    dial();\n    keepalive(30);\n}\n' }),
    ];
    return { worktree: wt, branch, base: 'main', mergeBase: '7fdd760aa', head: 'b2b2b2b2b2', revision: '1', truncated: false, readOnly: true, commits: [this.commit(1, 'Reconnect the SSH tunnel after sleep', 180)], recent: [], ...annotate(files) };
  }

  private sampleSide(wt: string): { branch: string; files: MockFile[]; commits: string[]; ago: number; agents: number } | null {
    if (wt === MOCK_WORKTREE || wt === MOCK_ROOT || wt === MOCK_SUBMODULE || wt.startsWith(MOCK_MISSING)) return null;
    const { lines: ls, tasks } = this.data();
    const task = tasks.find((x) => (x.worktreePath ?? ls.find((l) => l.line.id === x.lineId)?.line.worktreePath) === wt);
    const line = ls.find((l) => l.line.worktreePath === wt);
    if (!task && !line) return null;
    const title = task?.title ?? line?.local?.title ?? line?.line.branch ?? 'Sample change';
    const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'change';
    const path = `src/${slug}.ts`;
    return {
      branch: line?.line.branch ?? task?.branch ?? 'main',
      ago: 5,
      agents: 0,
      commits: [title],
      files: [{ path, oldPath: path, status: 'modified', base: `export function ${slug.replace(/-(\w)/g, (_, c: string) => c.toUpperCase())}() {\n  return 0;\n}\n`, now: `export function ${slug.replace(/-(\w)/g, (_, c: string) => c.toUpperCase())}() {\n  return 1;\n}\n` }],
    };
  }

  diff(wt: string, branch?: string | null): ReviewDiff {
    if (wt.startsWith(MOCK_MISSING)) throw new DaemonError(410, `the worktree folder ${MOCK_MISSING} no longer exists`);
    if (branch) return this.branchDiff(wt, branch);
    const side = this.side[wt] ?? this.linkedSide[wt] ?? this.sampleSide(wt);
    if (side) {
      const files = side.files.map((f) => this.fileJson(f));
      return { worktree: wt, branch: side.branch, base: 'main', mergeBase: '7fdd760aa', head: hash(wt), revision: hash(wt), truncated: false, commits: side.commits.map((m, i) => this.commit(i, m, side.ago + i * 5)), recent: [], ...annotate(files) };
    }
    if (wt === MOCK_SUBMODULE) {
      const files = [this.fileJson({ path: 'src/term.ts', oldPath: 'src/term.ts', status: 'modified', base: 'export const FPS = 30;\n', now: 'export const FPS = 60;\n' })];
      return { worktree: wt, branch: 'main', base: 'origin/main', baseSource: 'upstream', superproject: MOCK_WORKTREE, mergeBase: '3c1d9e2', head: '8f04b7c', revision: hash(wt), truncated: false, commits: [this.commit(0, 'Render at 60 fps', 30)], recent: [], ...annotate(files) };
    }
    if (wt !== MOCK_WORKTREE) {
      const main = wt === MOCK_ROOT;
      return { worktree: wt, branch: main ? 'main' : null, base: main ? 'origin/main' : 'main', mergeBase: main ? 'a5f7375' : null, head: main ? 'a5f7375' : null, revision: '0', truncated: false, commits: [], recent: main ? this.recentOnMain() : [], files: [] };
    }
    const sub: ReviewFile = { path: 'vendor/ghostty-web', oldPath: 'vendor/ghostty-web', status: 'submodule', binary: false, additions: 0, deletions: 0, hash: 'sub1', viewed: false, truncated: false, hunks: [], submodule: { from: '3c1d9e2a7b6f', to: '8f04b7c1d2e3' } };
    const files = [...this.files.map((f) => this.fileJson(f)), sub].sort((a, b) => a.path.localeCompare(b.path));
    const commits = ['Debounce PTY resizes', 'Move the debounce into resize.rs', 'Drop the legacy PTY module'].map((m, i) => this.commit(i, m, 9 + i * 20));
    return { worktree: wt, branch: 'feat/term-resize', base: 'main', baseSource: 'created', mergeBase: '9f1c2a7d0e', head: '4b2e81c', revision: hash(files.map((f) => f.hash).join()), truncated: false, commits, recent: [], linked: [{ worktree: MOCK_LINKED, branch: 'feat/term-resize' }], ...annotate(files) };
  }

  private find(id: string): { t: Thread; c?: ReviewComment } {
    for (const t of this.threads) {
      if (t.id === id) return { t };
      const c = t.comments.find((x) => x.id === id);
      if (c) return { t, c };
    }
    throw new DaemonError(404, `no comment ${id}`);
  }

  private save(t: Thread): Thread {
    t.updatedAt = new Date().toISOString();
    this.emit('review.thread', { thread: structuredClone(t) });
    return structuredClone(t);
  }

  route(method: string, full: string, body: any): unknown {
    const [path, qs] = full.split('?');
    const q = new URLSearchParams(qs ?? '');
    let m: RegExpMatchArray | null;
    if (method === 'GET' && path === '/v1/review') {
      const d = this.diff(this.worktreeOf(q), q.get('branch'));
      const base = q.get('base');
      return base && !d.readOnly ? { ...d, base: base.endsWith('~1') ? d.base : base, baseSource: base.endsWith('~1') ? d.baseSource : 'explicit' } : d;
    }
    if (method === 'GET' && path === '/v1/review/bases') return this.bases(this.worktreeOf(q));
    if (method === 'GET' && path === '/v1/review/map') {
      const plan: { wt: string; parent: number | null }[] = [];
      const push = (wt: string, parent: number | null) => {
        if (plan.some((x) => x.wt === wt)) return;
        const at = plan.length;
        plan.push({ wt, parent });
        if (parent === null) for (const f of this.diff(wt).files) if (f.status === 'submodule' && `${wt}/${f.path}` === MOCK_SUBMODULE) plan.push({ wt: MOCK_SUBMODULE, parent: at });
      };
      push(this.worktreeOf(q), null);
      for (const w of q.getAll('with')) {
        if (!w.startsWith('/')) throw new DaemonError(400, `${w} is not an absolute path`);
        if (w.startsWith(MOCK_MISSING)) throw new DaemonError(410, `the worktree folder ${w} no longer exists`);
        push(w.replace(/\/+$/, ''), null);
      }
      return mockMap(plan.map((p) => ({ diff: this.diff(p.wt), name: p.wt.split('/').pop() ?? p.wt, parent: p.parent, threads: this.threads.filter((t) => t.worktree === p.wt) })));
    }
    if (method === 'GET' && path === '/v1/review/graph') {
      const d = this.diff(this.worktreeOf(q), q.get('branch'));
      return mockGraph(d.files, d.revision);
    }
    if (method === 'GET' && path === '/v1/review/worktrees') {
      const wt = this.worktreeOf(q);
      if (wt.startsWith(MOCK_MISSING)) throw new DaemonError(410, `the worktree folder ${MOCK_MISSING} no longer exists`);
      return this.related(wt);
    }
    if (method === 'GET' && path === '/v1/review/branches') return this.branches();
    if (method === 'GET' && path === '/v1/review/file') {
      const f = this.files.find((x) => x.path === q.get('path'));
      if (!f || f.now === null) throw new DaemonError(404, `no file ${q.get('path')}`);
      return { path: f.path, lines: lines(f.now) };
    }
    if (method === 'PUT' && path === '/v1/review/viewed') {
      if (body.viewed) this.viewed[body.path] = body.hash;
      else delete this.viewed[body.path];
      return { path: body.path, hash: body.hash, viewed: !!body.viewed };
    }
    if (method === 'GET' && path === '/v1/review/comments') {
      const any = q.has('worktree') || q.has('line') || q.has('task');
      const wt = any ? this.worktreeOf(q) : null;
      const st = q.get('status')?.split(',');
      return structuredClone(this.threads.filter((t) => (!wt || t.worktree === wt) && (!st || st.includes(t.status))));
    }
    if (method === 'POST' && path === '/v1/review/comments') return this.create(body);
    if (method === 'POST' && path === '/v1/review/submit') return this.submit(body);
    if ((m = path.match(/^\/v1\/review\/comments\/([^/]+)\/apply$/)) && method === 'POST') return this.apply(decodeURIComponent(m[1]));
    if ((m = path.match(/^\/v1\/review\/comments\/([^/]+)$/))) {
      const id = decodeURIComponent(m[1]);
      if (method === 'PATCH') return this.patch(id, body);
      if (method === 'DELETE') return this.remove(id);
    }
    throw new DaemonError(404, `no route ${method} ${path}`);
  }

  private create(b: any): Thread {
    const body = String(b?.body ?? '').trim();
    if (!body) throw new DaemonError(400, 'body required');
    if (b.threadId) {
      const { t } = this.find(b.threadId);
      t.comments.push(this.comment('user', body, 0, true));
      return this.save(t);
    }
    const wt = this.worktreeOf(b);
    const side: Side = b.side === 'old' ? 'old' : 'new';
    const f = this.files.find((x) => x.path === b.path);
    const src = lines(side === 'old' ? (f?.base ?? null) : (f?.now ?? null));
    const line = Number(b.line);
    const end = Number(b.endLine ?? b.line);
    if (!f || line < 1 || end < line || end > src.length) throw new DaemonError(400, 'line outside the file');
    const t = this.thread(b.path, line, end, 'pending', [this.comment('user', body, 0, true)], { worktree: wt, side, code: src.slice(line - 1, end).join('\n') });
    this.threads.push(t);
    return this.save(t);
  }

  private patch(id: string, b: any): Thread {
    const { t, c } = this.find(id);
    if (c) {
      if (!c.pending) throw new DaemonError(409, 'only pending comments can be edited');
      c.body = String(b.body ?? '');
      c.updatedAt = new Date().toISOString();
      return this.save(t);
    }
    if (b.status === 'resolved') t.status = 'resolved';
    else if (b.status === 'open') t.status = t.delivery || t.comments.some((x) => !x.pending) ? 'sent' : 'pending';
    else if (b.status === 'addressed') {
      t.status = 'addressed';
      if (b.note) t.comments.push(this.comment('agent', String(b.note), 0));
    } else throw new DaemonError(400, 'status must be resolved, open or addressed');
    return this.save(t);
  }

  private remove(id: string): { deleted: string } {
    const { t, c } = this.find(id);
    if (c && !c.pending) throw new DaemonError(409, 'not pending');
    if (!c && t.status !== 'pending') throw new DaemonError(409, 'not pending');
    if (!c || t.comments.length === 1) {
      this.threads = this.threads.filter((x) => x !== t);
      this.emit('review.thread_deleted', { id: t.id, worktree: t.worktree });
      return { deleted: t.id };
    }
    t.comments = t.comments.filter((x) => x !== c);
    this.save(t);
    return { deleted: id };
  }

  private apply(id: string): Thread {
    const { t } = this.find(id);
    const s = threadSuggestion(t);
    const f = this.files.find((x) => x.path === t.path);
    if (!s || !f || f.now === null || t.outdated) throw new DaemonError(409, 'no suggestion to apply');
    const cur = lines(f.now);
    if (cur.slice(t.line - 1, t.endLine).join('\n') !== t.code) throw new DaemonError(409, 'the lines changed since the comment');
    f.now = [...cur.slice(0, t.line - 1), ...s, ...cur.slice(t.endLine)].join('\n') + '\n';
    t.status = 'resolved';
    t.code = s.join('\n');
    t.endLine = t.line + s.length - 1;
    this.emit('review.changed', { worktree: t.worktree, files: [t.path], revision: this.diff(t.worktree).revision });
    return this.save(t);
  }

  private submit(b: any) {
    const wt = this.worktreeOf(b);
    const kind: SubmitKind = b.kind;
    if (!['comment', 'request_changes', 'approve'].includes(kind)) throw new DaemonError(400, 'kind must be comment, request_changes or approve');
    const sent = this.threads.filter((t) => t.worktree === wt && t.comments.some((c) => c.pending));
    const text = this.instruction(kind, sent, b.body);
    if (!sent.length && !b.body) return { delivery: this.delivery('none', 0, [], kind), threads: [], text: '' };
    const d = this.delivery('queued', 0, sent.map((t) => t.id), kind);
    for (const t of sent) {
      for (const c of t.comments) c.pending = false;
      if (t.status === 'pending' || t.status === 'addressed') t.status = 'sent';
      t.delivery = d;
      this.save(t);
    }
    this.emit('review.delivery', { delivery: structuredClone(d), worktree: wt });
    this.timers.push(
      setTimeout(() => {
        d.status = 'delivered';
        d.reason = null;
        d.at = new Date().toISOString();
        this.emit('review.delivery', { delivery: structuredClone(d), worktree: wt });
      }, 1600),
    );
    return { delivery: structuredClone(d), threads: structuredClone(sent), text };
  }

  instruction(kind: SubmitKind, threads: Thread[], body?: string): string {
    const head = kind === 'request_changes' ? 'changes requested' : kind === 'approve' ? 'approved' : 'comments';
    const files = new Set(threads.map((t) => t.path)).size;
    const out = [`Code review: ${head} on ${files} files (${threads.length} comments).`];
    threads.forEach((t, i) => {
      out.push('', `${i + 1}. ${t.path}:${t.line}${t.endLine !== t.line ? `-${t.endLine}` : ''} (id ${t.id})`);
      for (const l of t.code.split('\n').slice(0, 12)) out.push(`   > ${l.trim()}`);
      for (const c of t.comments.filter((x) => x.pending)) out.push(`   ${c.body.replace(/\n/g, '\n   ')}`);
    });
    if (body) out.push('', `Overall: ${body}`);
    return out.join('\n');
  }

  stop() {
    for (const t of this.timers) clearTimeout(t);
    this.timers = [];
  }
}
