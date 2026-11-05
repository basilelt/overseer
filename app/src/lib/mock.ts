import { mockReviewRequests } from './github/mock';
import { mockUsage } from './usage/mock';
import { DaemonError, type DaemonClient, type TermIO } from './client';
import { phoneParam } from './mobile/layout';
import { MockGit } from './git/mockGit';
import { bigDiff, bigPatch, MockReview } from './review/mock';
import { chatPage, mockScreen, MOCK_COMMANDS, seedAgentChat, seedChat } from './chat/mock';
import type { Effort, HookResult, HooksStatus, SwitchboardStatus, Profile, ProfileChip, ProfilesStore, SourceItem, SourceItems, SubagentDef, TaskSource } from './types';
import { mockSystem } from './system/mock';
import { resolveProfile } from './profiles';
import { fileKind } from './files';
import { cleanTitle } from './titles';
import type { DirListing } from './dirs';
import type { Action, Proposal, Result } from './organize';
import type {
  DeviceKey,
  PlatformInfo,
  SavedHost,
  LocalNetworkReport,
  BroadcastRequest,
  BroadcastResponse,
  ChatEntry,
  BroadcastResult,
  Folder,
  Group,
  Workspace,
  Activity,
  Agent,
  AgentInfo,
  ConnPhase,
  ConnState,
  DaemonEvent,
  Diff,
  EventType,
  Forward,
  HostTarget,
  LocalLine,
  NewTask,
  TaskMessage,
  NewTerm,
  Port,
  PortSettings,
  PortsState,
  Project,
  SshHost,
  Task,
  TaskStatus,
  Term,
  UploadProgress,
  UploadResult,
  Lifecycle,
  LifecyclePatch,
} from './types';

const MIN = 60_000;

const HOME = '/home/alice';
const ROOT = HOME;
const REPOS: Record<string, { path: string; repo: string }> = {
  webshop: { path: `${HOME}/Documents/Github/work/webshop`, repo: 'github.com/acme/webshop' },
  'overseer': { path: `${HOME}/Documents/Github/work/overseer`, repo: 'github.com/basilelt/overseer' },
  'infra-lab': { path: `${HOME}/infra-lab`, repo: 'github.com/alice/infra-lab' },
  compilers: { path: `${HOME}/Documents/Github/uni/compilers`, repo: 'github.com/uni/compilers' },
  'os-labs': { path: `${HOME}/Documents/Github/uni/os-labs`, repo: 'github.com/uni/os-labs' },
};
const repoPath = (name: string) => REPOS[name].path;

const ALL_EFFORTS = ['low', 'medium', 'high', 'max'] as const;

export const MOCK_AGENTS: AgentInfo[] = [
  { id: 'claude', name: 'Claude Code', installed: true, version: '2.1.4', models: ['opus', 'sonnet', 'haiku', 'fable'], efforts: [...ALL_EFFORTS], resume: true, verified: true, sessions: true, turns: true },
  { id: 'codex', name: 'Codex', installed: true, version: '0.58.0', efforts: [...ALL_EFFORTS], resume: true, verified: true, sessions: true, turns: true },
  { id: 'gemini', name: 'Gemini CLI', installed: true, version: '0.21.0', efforts: [], resume: true, verified: true, sessions: true, turns: false },
  { id: 'agy', name: 'Antigravity', installed: true, version: '1.4.2', efforts: [], resume: false, verified: true, sessions: false, turns: false },
  { id: 'opencode', name: 'opencode', installed: false, efforts: [], resume: true, verified: true, install: 'npm install -g --allow-scripts=opencode-ai opencode-ai', sessions: false, turns: false },
  { id: 'amp', name: 'Amp', installed: false, models: [], efforts: [...ALL_EFFORTS], resume: true, verified: true, install: 'npm install -g --allow-scripts=@ampcode/cli @sourcegraph/amp', sessions: false, turns: false },
  { id: 'copilot', name: 'Copilot CLI', installed: true, version: '0.0.354', efforts: [...ALL_EFFORTS], resume: true, verified: true, sessions: false, turns: false },
  { id: 'cursor', name: 'Cursor Agent', installed: false, efforts: [], resume: true, verified: false, install: 'curl https://cursor.com/install -fsS | bash', sessions: false, turns: false },
  { id: 'shell', name: 'Shell', installed: true, models: [], efforts: [], resume: false, verified: true, sessions: false, turns: false },
];

export const MOCK_PROFILES: ProfilesStore = {
  default: '',
  scopes: {},
  profiles: [
    {
      id: 'economy',
      name: 'Economy',
      builtin: true,
      agents: {
        claude: { model: 'sonnet', effort: 'low', subagentModel: 'haiku', subagentForce: true },
        codex: { model: 'gpt-5.4-mini', effort: 'low', subagentModel: 'gpt-5.4-mini', subagentEffort: 'low' },
        copilot: { model: 'gpt-5-mini', effort: 'low' },
        gemini: { model: 'gemini-2.5-flash' },
        amp: { effort: 'low' },
      },
    },
    {
      id: 'balanced',
      name: 'Balanced',
      builtin: true,
      agents: {
        claude: { model: 'sonnet', effort: 'medium', subagentModel: 'sonnet' },
        codex: { model: 'gpt-5.4', effort: 'medium', subagentModel: 'gpt-5.4-mini', subagentEffort: 'medium' },
        copilot: { effort: 'medium' },
        amp: { effort: 'medium' },
      },
    },
    {
      id: 'max',
      name: 'Max',
      builtin: true,
      agents: { claude: { model: 'opus', effort: 'max' }, codex: { model: 'gpt-5.5', effort: 'max' }, copilot: { effort: 'max' }, amp: { effort: 'max' } },
    },
  ],
  support: {
    claude: { model: 'flag', effort: 'flag', subagentModel: 'env', smallModel: 'env', subagents: 'file', subagentEfforts: true, subagentForce: true },
    codex: { model: 'flag', effort: 'flag', subagentModel: 'flag', subagentEffort: 'flag' },
    gemini: { model: 'flag' },
    agy: { model: 'flag' },
    opencode: { model: 'flag', smallModel: 'env', subagents: 'env' },
    amp: { effort: 'flag' },
    copilot: { model: 'flag', effort: 'flag' },
    cursor: { model: 'flag' },
  },
};

export const MOCK_SUBAGENTS: SubagentDef[] = [
  { agent: 'claude', name: 'code-reviewer', scope: 'project', path: `${HOME}/Documents/Github/work/overseer/.claude/agents/code-reviewer.md`, model: 'opus', effort: '' },
  { agent: 'claude', name: 'debug-specialist', scope: 'user', path: `${HOME}/.claude/agents/debug-specialist.md`, model: 'opus', effort: '' },
  { agent: 'claude', name: 'test-writer', scope: 'user', path: `${HOME}/.claude/agents/test-writer.md`, model: 'sonnet', effort: 'high' },
];

const agentInfo = (id: string) => MOCK_AGENTS.find((a) => a.id === id);

export const MOCK_PROJECTS: Project[] = Object.entries(REPOS).map(([name, r]) => ({ path: r.path, name, repo: r.repo, defaultBranch: 'main' }));

interface Seed {
  id: string;
  agent: Agent;
  repo: string;
  branch: string;
  ticket?: string;
  title: string;
  goal: string;
  status: string;
  state: LocalLine['line']['state'];
  busy?: boolean;
  needsInput?: boolean;
  pr?: boolean;
  ago: number;
}

const SEEDS: Seed[] = [
  { id: 'ln_01', agent: 'claude', repo: 'webshop', branch: 'feat/proj-10844-docker-images', ticket: 'PROJ-10844', title: 'PROJ-10844 docker images multi-arch', goal: 'Build arm64 and amd64 images in one pipeline', status: 'Asking: push the arm64 tag to the staging registry too?', state: 'active', needsInput: true, ago: 1 },
  { id: 'ln_02', agent: 'codex', repo: 'webshop', branch: 'fix/proj-10851-stock-sync', ticket: 'PROJ-10851', title: 'PROJ-10851 stock sync retries', goal: 'Retry the stock sync on 503 with backoff', status: 'Permission needed: run make test', state: 'active', needsInput: true, ago: 3 },
  { id: 'ln_03', agent: 'claude', repo: 'overseer', branch: 'feat/term-resize', title: 'PTY resize debounce', goal: 'Debounce terminal resize events', status: 'Ready for review: 4 files changed', state: 'active', ago: 9 },
  { id: 'ln_04', agent: 'claude', repo: 'overseer', branch: 'feat/workspaces', title: 'Workspaces sidebar', goal: 'One project, multiple tabs inside', status: 'Running cargo test (212 passed so far)', state: 'active', busy: true, ago: 0 },
  { id: 'ln_05', agent: 'codex', repo: 'webshop', branch: 'refactor/proj-10802-orders', ticket: 'PROJ-10802', title: 'Orders service split', goal: 'Extract invoicing from the orders service', status: 'Editing src/invoicing/mod.rs', state: 'active', busy: true, ago: 0 },
  { id: 'ln_06', agent: 'gemini', repo: 'os-labs', branch: 'lab/scheduler', title: 'Scheduler lab report', goal: 'Compare round-robin and CFS on the lab traces', status: 'Reading traces/cfs.csv', state: 'active', busy: true, ago: 2 },
  { id: 'ln_07', agent: 'claude', repo: 'infra-lab', branch: 'feat/talos-node2', title: 'Talos OS installation on node2', goal: 'Install Talos on the node2 node and join the cluster', status: 'Claude finished', state: 'active', pr: true, ago: 35 },
  { id: 'ln_08', agent: 'codex', repo: 'webshop', branch: 'fix/proj-10819-vat', ticket: 'PROJ-10819', title: 'VAT rounding', goal: 'Round VAT per line, not per order', status: 'PR #212 open, review requested', state: 'idle', pr: true, ago: 70 },
  { id: 'ln_09', agent: 'claude', repo: 'overseer', branch: 'docs/user-guide', title: 'User guide pass', goal: 'Proofread the remote mode chapter', status: 'Done. Waiting for next instruction.', state: 'idle', ago: 48 },
  { id: 'ln_10', agent: 'copilot', repo: 'compilers', branch: 'feat/parser-recovery', title: 'Parser error recovery', goal: 'Recover after a missing semicolon', status: 'At prompt', state: 'idle', ago: 140 },
  { id: 'ln_11', agent: 'codex', repo: 'infra-lab', branch: 'feat/ceph-autoscale', title: 'Ceph pool autoscale', goal: 'Enable pg autoscale on the rbd pools', status: 'Held by you', state: 'held', ago: 300 },
  { id: 'ln_12', agent: 'claude', repo: 'webshop', branch: 'spike/graphql', title: 'GraphQL spike', goal: 'Evaluate async-graphql for the BFF', status: 'Process lost after reboot', state: 'held', ago: 1440 },
];

function iso(minsAgo: number, now = Date.now()) {
  return new Date(now - minsAgo * MIN).toISOString();
}

function seedLines(now: number): LocalLine[] {
  return SEEDS.map((s, i) => ({
    line: {
      id: s.id,
      owner: 'alice',
      ticketKey: s.ticket ?? null,
      stationId: 'atlas',
      agent: s.agent,
      repo: REPOS[s.repo].repo,
      branch: s.branch,
      worktreePath: `/home/alice/worktrees/${s.repo}/${s.branch.replace('/', '-')}`,
      prUrl: s.pr ? `https://${REPOS[s.repo].repo}/pull/${80 + i * 11}` : null,
      state: s.state,
      lastActivityAt: iso(s.ago, now),
    },
    local: {
      pid: s.state === 'held' ? undefined : 4100 + i,
      busy: !!s.busy,
      needsInput: !!s.needsInput,
      title: s.title,
      heldReason: s.state === 'held' ? (s.ago > 600 ? 'lost' : 'user') : undefined,
    },
  }));
}

const SCRATCH = '/srv/scratch';
const SCRATCH_REPOS = ['scrapers', 'infra-notes', 'llm-evals'];

function scatteredLines(now: number): LocalLine[] {
  const agents: Agent[] = ['claude', 'codex', 'gemini'];
  return Array.from({ length: 18 }, (_, i) => {
    const repo = SCRATCH_REPOS[i % SCRATCH_REPOS.length];
    return {
      line: {
        id: `ln_s${String(i + 1).padStart(2, '0')}`,
        owner: 'alice',
        stationId: 'atlas',
        agent: agents[i % agents.length],
        repo: `local:${SCRATCH}/${repo}`,
        branch: `try/${repo}-${i + 1}`,
        worktreePath: `${SCRATCH}/${repo}/wt-${i + 1}`,
        state: 'idle',
        lastActivityAt: iso(60 * (6 + i * 7), now),
      },
      local: { title: `${repo} experiment ${i + 1}` },
    };
  });
}

function seedActivity(): Record<string, Activity> {
  const out: Record<string, Activity> = {};
  for (const s of SEEDS) {
    const running = !!s.busy;
    out[s.id] = {
      title: s.title,
      goal: s.goal,
      lineId: s.id,
      busy: running,
      status: s.state,
      needsInput: !!s.needsInput,
      workLog: [
        { kind: 'read', label: 'Read project layout', state: 'done', at: iso(s.ago + 12) },
        { kind: 'read', label: 'Read src/lib.rs', path: 'src/lib.rs', state: 'done', at: iso(s.ago + 10) },
        { kind: 'edit', label: 'Edit src/handlers.rs', path: 'src/handlers.rs', state: 'done', at: iso(s.ago + 6) },
        { kind: s.needsInput ? 'other' : 'bash', label: s.needsInput ? 'cargo check' : s.status, state: running ? 'running' : 'done', at: iso(s.ago + 1) },
      ],
      todos: [
        { text: 'Understand the current flow', state: 'completed' },
        { text: s.goal, state: running ? 'in_progress' : 'completed' },
        { text: 'Add tests', state: running ? 'pending' : 'completed' },
        { text: 'Update the changelog', state: 'pending' },
      ],
      editedFiles: ['src/handlers.rs', 'src/model.rs', 'tests/handlers.rs'],
      lastAssistantText: s.needsInput
        ? `${s.status} I can do either; the cap before shipping matches the spec in ${s.ticket ?? 'the ticket'}.`
        : `${s.status}.`,
    };
  }
  return out;
}

const TASK_SEEDS: [string, string, string, TaskStatus, Agent, string, string?, string?][] = [
  ['Rate-limit the public search endpoint', 'Token bucket per API key, 429 with Retry-After.', 'webshop', 'pending', 'claude', 'opus'],
  ['Dark mode for the admin', 'Follow prefers-color-scheme, persist override.', 'webshop', 'pending', 'copilot', ''],
  ['Flaky checkout e2e', 'Stabilise the payment iframe wait in checkout.spec.ts.', 'webshop', 'pending', 'claude', 'sonnet', 'PROJ-10837'],
  ['Terraform drift report', 'Nightly plan on every workspace, post the diff.', 'infra-lab', 'auto', 'claude', 'sonnet'],
  ['Retry webhooks with jitter', 'Exponential backoff with full jitter, cap at 10 min.', 'overseer', 'auto', 'codex', 'gpt-5-codex'],
  ['Product image CDN migration', 'Move product images to the new bucket and rewrite URLs.', 'webshop', 'auto', 'gemini', 'gemini-2.5-pro', 'PROJ-10840'],
  ['Workspaces sidebar', 'One project, multiple tabs inside.', 'overseer', 'in_progress', 'claude', 'opus', undefined, 'ln_04'],
  ['Orders service split', 'Extract invoicing from the orders service.', 'webshop', 'in_progress', 'codex', 'gpt-5-codex', 'PROJ-10802', 'ln_05'],
  ['Scheduler lab report', 'Compare round-robin and CFS on the lab traces.', 'os-labs', 'in_progress', 'gemini', '', undefined, 'ln_06'],
  ['PTY resize debounce', 'Debounce terminal resize events to 50 ms.', 'overseer', 'in_testing', 'claude', 'sonnet', undefined, 'ln_03'],
  ['VAT rounding', 'Round VAT per line, not per order.', 'webshop', 'in_testing', 'codex', 'gpt-5-codex', 'PROJ-10819', 'ln_08'],
  ['Talos OS installation on node2', 'Install Talos on the node2 node and join the cluster.', 'infra-lab', 'completed', 'claude', '', undefined, 'ln_07'],
  ['User guide pass', 'Proofread the remote mode chapter.', 'overseer', 'completed', 'claude', 'haiku', undefined, 'ln_09'],
  ['Sentry release tagging', 'Tag releases with the git sha in CI.', 'webshop', 'completed', 'codex', 'o4-mini'],
];

function seedTasks(now: number): Task[] {
  const own: Task[] = TASK_SEEDS.map(([title, description, name, status, agent, model, ticketKey, lineId], i) => ({
    rank: TASK_SEEDS.slice(0, i).filter((x) => x[3] === status).length + 1,
    id: `tk_${String(i + 1).padStart(2, '0')}`,
    number: 101 + i,
    title,
    description,
    project: repoPath(name),
    status,
    agent,
    model,
    effort: agentInfo(agent)?.efforts.length ? (['medium', 'high', 'max', 'low'] as const)[i % 4] : 'high',
    station: 'atlas',
    ticketKey: ticketKey ?? null,
    lineId: lineId ?? null,
    termId: lineId ? `tm_${lineId}` : null,
    branch: lineId ? SEEDS.find((s) => s.id === lineId)?.branch : null,
    progress: status === 'in_progress' ? { done: 1 + (i % 3), total: 5, note: 'Implementing' } : status === 'in_testing' || status === 'completed' ? { done: 5, total: 5 } : null,
    createdAt: iso(60 * 24 * (TASK_SEEDS.length - i) / 3, now),
    updatedAt: iso(30 - i, now),
  }));
  const quadrants: Record<string, [boolean, boolean]> = {
    tk_01: [true, true],
    tk_02: [false, false],
    tk_03: [false, true],
    tk_04: [false, true],
    tk_05: [true, false],
    tk_07: [true, true],
    tk_08: [false, true],
    tk_10: [true, false],
  };
  const deps: Record<string, string[]> = { tk_06: ['tk_08'], tk_03: ['tk_01', 'tk_02'] };
  for (const t of own) {
    const q = quadrants[t.id];
    if (q) [t.urgent, t.important] = q;
    t.dependsOn = deps[t.id] ?? [];
  }
  return own.concat(seedAgentTasks(now), seedSessionTasks(now));
}

const AGENT_TASK_SEEDS: [string, string, string, boolean | null, boolean | null][] = [
  ['Restore flaky workspace test', 'workspaces.test.ts fails one run in ten: the restore order depends on a timer.', 'tk_07', false, true],
  ['Crash on empty folder name', 'Creating a folder with an empty name panics in the registry.', 'tk_08', true, true],
  ['Remove dead resize helper', 'term::resize_legacy has no callers left.', 'tk_07', false, false],
  ['Document the shared id type', 'The new id type in ids.rs has no doc comment or test.', 'tk_08', null, null],
];

function seedAgentTasks(now: number): Task[] {
  return AGENT_TASK_SEEDS.map(([title, description, parentId, urgent, important], i): Task => ({
    id: `tk_a${i + 1}`,
    number: 118 + i,
    title,
    description,
    project: repoPath(parentId === 'tk_07' ? 'overseer' : 'webshop'),
    status: 'pending',
    agent: 'claude',
    model: 'sonnet',
    effort: 'medium',
    rank: 4 + i,
    station: 'atlas',
    origin: 'agent',
    parentId,
    ...(urgent === null ? {} : { urgent, important }),
    createdAt: iso(40 - i * 7, now),
    updatedAt: iso(40 - i * 7, now),
  }));
}

function seedMessages(tasks: Task[], now: number): TaskMessage[] {
  const num = (id: string) => tasks.find((t) => t.id === id);
  const from = (id: string) => ({ from: id, fromNumber: num(id)?.number, fromTitle: num(id)?.title });
  return [
    { id: 'msg_1', taskId: 'tk_07', ...from('tk_08'), body: 'I am about to edit src/workspace.rs for the shared id type. Which of us changes it?', createdAt: iso(26, now), delivered: true },
    { id: 'msg_2', taskId: 'tk_08', ...from('tk_07'), body: 'I own src/workspace.rs. Take the id type in src/ids.rs, I will rebase on it.', createdAt: iso(24, now), delivered: true },
    { id: 'msg_3', taskId: 'tk_07', from: null, body: 'Create a task for anything unrelated you find instead of fixing it here.', createdAt: iso(18, now), delivered: true },
  ];
}

function seedSessionTasks(now: number): Task[] {
  return SEEDS.filter((s) => s.id === 'ln_01' || s.id === 'ln_02').map((s, i): Task => ({
    id: `tk_s${i + 1}`,
    number: 101 + TASK_SEEDS.length + i,
    title: s.title,
    description: s.goal,
    project: repoPath(s.repo),
    status: 'in_progress' as const,
    agent: s.agent,
    model: s.agent === 'claude' ? 'claude-opus-5-5' : 'gpt-5-codex',
    effort: 'high' as const,
    rank: 100 + i,
    station: 'atlas',
    ticketKey: s.ticket ?? null,
    lineId: s.id,
    branch: s.branch,
    worktreePath: `${repoPath(s.repo)}/.claude/worktrees/${s.branch.split('/').pop()}`,
    origin: 'session' as const,
    badges: s.needsInput ? ['needs_input' as const] : [],
    progress: i === 0 ? { done: 2, total: 4, note: 'Push the arm64 tag' } : { done: 0, total: 0, note: 'Editing src/stock/sync.rs' },
    createdAt: iso(60 * 3, now),
    updatedAt: iso(s.ago, now),
  })).concat({
    id: 'tk_s3',
    number: 103 + TASK_SEEDS.length,
    title: 'New Claude Code session',
    description: '',
    project: repoPath('overseer'),
    status: 'in_progress',
    agent: 'claude',
    model: 'claude-opus-5-5',
    effort: 'high',
    rank: 102,
    station: 'atlas',
    ticketKey: null,
    lineId: 'ln_13',
    branch: 'main',
    origin: 'session',
    badges: ['needs_input'],
    progress: null,
    createdAt: iso(4, now),
    updatedAt: iso(2, now),
  });
}

function seedTerms(lines: LocalLine[], now: number): Term[] {
  const live = lines.filter((l) => (l.line.state === 'active' || !agentInfo(l.line.agent)?.sessions) && l.line.state !== 'held').slice(0, 8);
  const terms: Term[] = live.map((l) => ({
    id: `tm_${l.line.id}`,
    pid: (l.local?.pid ?? 0) + 1000,
    cwd: l.line.worktreePath ?? ROOT,
    cmd: [l.line.agent, '--resume', l.line.id],
    title: l.local?.title ?? l.line.id,
    cols: 120,
    rows: 32,
    createdAt: l.line.lastActivityAt,
    exited: false,
    lineId: l.line.id,
    agent: l.line.agent,
  }));
  terms.push({ id: 'tm_shell1', pid: 5200, cwd: `${HOME}/Documents/Github/work/overseer`, cmd: ['zsh'], title: 'zsh', cols: 120, rows: 32, createdAt: iso(20, now), exited: false, agent: 'shell' });
  terms.push({ id: 'tm_shell2', pid: 5201, cwd: HOME, cmd: ['zsh'], title: 'zsh', cols: 120, rows: 32, createdAt: iso(50, now), exited: false, agent: 'shell' });
  return terms;
}

const DIFF = `diff --git a/src/term.rs b/src/term.rs
index 3f2a1c0..9b7e4d2 100644
--- a/src/term.rs
+++ b/src/term.rs
@@ -12,14 +12,22 @@ use std::time::{Duration, Instant};
 pub struct Term {
     pty: Pty,
-    size: (u16, u16),
+    size: (u16, u16),
+    pending: Option<(u16, u16)>,
+    last_resize: Instant,
 }

 impl Term {
-    pub fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
-        self.pty.resize(cols, rows)
+    pub fn resize(&mut self, cols: u16, rows: u16) {
+        self.pending = Some((cols, rows));
+    }
+
+    pub fn flush_resize(&mut self) -> io::Result<()> {
+        if self.last_resize.elapsed() < Duration::from_millis(50) {
+            return Ok(());
+        }
+        let Some((cols, rows)) = self.pending.take() else { return Ok(()) };
+        self.last_resize = Instant::now();
+        self.pty.resize(cols, rows)
     }
 }
diff --git a/tests/term.rs b/tests/term.rs
index 1111111..2222222 100644
--- a/tests/term.rs
+++ b/tests/term.rs
@@ -40,3 +40,12 @@ fn spawns_shell() {
     assert!(t.pid() > 0);
 }
+
+#[test]
+fn resize_is_debounced() {
+    let mut t = Term::spawn(&["sh"]).unwrap();
+    t.resize(80, 24);
+    t.resize(100, 30);
+    t.flush_resize().unwrap();
+    assert_eq!(t.size(), (100, 30));
+}
`;

const FOLDER_SEEDS: [string, string, boolean][] = [
  ['f_uni', 'Uni', true],
  ['f_alice', 'alice', false],
  ['f_acme', 'acme', false],
];

const WS_SEEDS: [string, string, string, string | null, boolean, boolean, boolean?][] = [
  ['ws_compilers', 'compilers', repoPath('compilers'), 'f_uni', false, true],
  ['ws_oslabs', 'os-labs', repoPath('os-labs'), 'f_uni', false, true],
  ['ws_root', 'root', HOME, 'f_alice', false, false],
  ['ws_proxmox', 'infra-lab', repoPath('infra-lab'), 'f_alice', false, true],
  ['ws_webshop', 'webshop', repoPath('webshop'), 'f_acme', true, true],
  ['ws_overseer', 'overseer', repoPath('overseer'), 'f_acme', false, true],
  ['ws_thesis', 'thesis', `${HOME}/Documents/thesis`, 'f_alice', false, false, true],
];

const DISCOVERABLE: Project[] = [{ path: `${HOME}/Documents/Github/perso/dotfiles`, name: 'dotfiles', repo: 'github.com/alice/dotfiles' }];

function seedWs(now: number): { workspaces: Workspace[]; folders: Folder[]; groups: Group[] } {
  const at = iso(60 * 24, now);
  return {
    folders: FOLDER_SEEDS.map(([id, name, collapsed], i) => ({ id, name, parentId: null, rank: i + 1, collapsed, createdAt: at, updatedAt: at })),
    workspaces: WS_SEEDS.map(([id, name, path, folderId, pinned, auto, missing], i) => ({
      id,
      name,
      path,
      station: 'atlas',
      folderId,
      pinned,
      archived: false,
      auto,
      rank: i + 1,
      tabs: [],
      notifyIdle: false,
      ...(missing ? { missing } : {}),
      createdAt: at,
      updatedAt: at,
    })),
    groups: [
      { id: 'g_release', name: 'release 5.8', rank: 1, members: ['line:ln_01', 'line:ln_05', 'line:ln_08'], notifyIdle: false, createdAt: at, updatedAt: at },
      { id: 'g_infra', name: 'infra', rank: 2, members: ['line:ln_07', 'line:ln_11', 'line:ln_04'], notifyIdle: false, createdAt: at, updatedAt: at },
    ],
  };
}

const enc = new TextEncoder();

type WsKind = 'workspaces' | 'folders' | 'groups';

interface MockOptions {
  live?: boolean;
  scattered?: boolean;
  delayMs?: number;
  scale?: number;
  storm?: number;
  platform?: PlatformInfo;
  switchboard?: boolean;
}

const urlNumber = (k: string) => (typeof location !== 'undefined' ? Number(new URLSearchParams(location.search).get(k)) || 0 : 0);

function scaledLines(n: number, now: number): LocalLine[] {
  const agents: Agent[] = ['claude', 'codex', 'gemini'];
  const repos = Object.keys(REPOS);
  return Array.from({ length: n }, (_, i) => {
    const repo = repos[i % repos.length];
    const state = i % 7 === 0 ? 'held' : 'active';
    return {
      line: {
        id: `ln_x${i}`,
        owner: 'alice',
        stationId: 'atlas',
        agent: agents[i % agents.length],
        repo: REPOS[repo].repo,
        branch: `feat/scale-${i}`,
        worktreePath: `/home/alice/worktrees/${repo}/scale-${i}`,
        prUrl: i % 5 === 0 ? `https://${REPOS[repo].repo}/pull/${1000 + i}` : null,
        state,
        lastActivityAt: iso(i, now),
      },
      local: { pid: 9000 + i, busy: i % 3 === 0, needsInput: i % 11 === 0, title: `Scaled session ${i}` },
    };
  });
}

function scaledTasks(n: number, now: number): Task[] {
  const statuses: TaskStatus[] = ['pending', 'auto', 'in_progress', 'in_testing', 'completed'];
  const repos = Object.keys(REPOS);
  return Array.from({ length: n }, (_, i) => ({
    id: `tk_x${i}`,
    number: 1000 + i,
    title: `Scaled task ${i}`,
    description: `Generated task ${i} for load testing.`,
    project: repoPath(repos[i % repos.length]),
    status: statuses[i % statuses.length],
    agent: 'claude',
    model: 'sonnet',
    effort: 'medium',
    rank: 1000 + i,
    station: 'atlas',
    ticketKey: null,
    lineId: null,
    termId: null,
    progress: null,
    createdAt: iso(i * 3, now),
    updatedAt: iso(i, now),
  }));
}

export const MOCK_SAVED_HOSTS: SavedHost[] = [
  { id: 'h1', label: 'Workstation', host: 'alice-workstation.lan', port: 22, user: 'alice', auth: 'key', rememberPassword: false },
  {
    id: 'h2',
    label: 'GPU box',
    host: '10.0.4.30',
    port: 22,
    user: 'alice',
    jump: { host: 'bastion.example.com', port: 22, user: 'alice' },
    auth: 'key',
    rememberPassword: false,
  },
];

const MOCK_PNG = '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';

const utf8 = (s: string) => new TextEncoder().encode(s).buffer as ArrayBuffer;

function mockSvg(file: string): string {
  const name = file.slice(file.lastIndexOf('/') + 1).replace(/[<&]/g, '');
  return `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="400" viewBox="0 0 640 400"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#6366f1"/><stop offset="1" stop-color="#14b8a6"/></linearGradient></defs><rect width="640" height="400" fill="url(#g)"/><circle cx="500" cy="110" r="60" fill="#fde68a"/><path d="M0 400 L200 220 L330 330 L440 250 L640 400Z" fill="#0f172a" opacity=".55"/><text x="32" y="60" font-family="sans-serif" font-size="28" fill="#fff">${name}</text></svg>`;
}

export class MockDaemon implements DaemonClient {
  readonly kind = 'mock' as const;
  lines: LocalLine[];
  terms: Term[];
  tasks: Task[];
  autoOrganize = false;
  messages: TaskMessage[];
  activity: Record<string, Activity>;
  projects: Project[] = structuredClone(MOCK_PROJECTS);
  taskSources: (TaskSource & { project: string })[] = [];
  agents: AgentInfo[] = structuredClone(MOCK_AGENTS);
  hooks: HooksStatus = mockHooks();
  switchboard: SwitchboardStatus = { found: false, dir: null, imported: false, running: false, sessions: 0, tasks: 0, workspaces: 0, hooks: false, settings: {} };
  statusline = { path: '~/.claude/settings.json', installed: false, command: '~/.claude/statusline.sh' as string | null };
  profileStore: ProfilesStore = structuredClone(MOCK_PROFILES);
  git = new MockGit(this);
  workspaces: Workspace[];
  folders: Folder[];
  groups: Group[];
  queued: { id: string; target: string; text: string; at: number }[] = [];
  delivered: { target: string; text: string }[] = [];
  private subs = new Set<(e: DaemonEvent) => void>();
  review = new MockReview((t, d) => this.emit(t, d), () => ({ lines: this.lines, tasks: this.tasks }));
  private seq = 0;
  private timers: ReturnType<typeof setInterval>[] = [];
  private live: boolean;
  private delayMs: number;
  private target: HostTarget = { kind: 'local', label: 'Local' };
  private portWatchers = new Set<(s: PortsState) => void>();
  private portTimers: ReturnType<typeof setTimeout>[] = [];
  portConfig: PortSettings = { autoForward: true, ignore: [22], localhostOnly: true };
  detected: Port[] = [];
  forwards: Forward[] = [];
  private unforwarded = new Set<number>();
  private prompts = new Map<number, (answer: string | null) => void>();
  private promptSeq = 0;
  hosts: SavedHost[] = structuredClone(MOCK_SAVED_HOSTS);
  private trusted = new Set<string>(['h1']);
  private devicePlatform: PlatformInfo | undefined;
  private sinks = new Map<string, (s: string) => void>();

  constructor(opts: MockOptions = {}) {
    const now = Date.now();
    const scattered = opts.scattered ?? (typeof location !== 'undefined' && new URLSearchParams(location.search).has('scattered'));
    const scale = opts.scale ?? urlNumber('scale');
    this.storm = opts.storm ?? urlNumber('storm');
    this.lines = [...seedLines(now), ...(scattered ? scatteredLines(now) : []), ...scaledLines(scale, now)];
    this.terms = seedTerms(this.lines, now);
    if (scale) this.terms.push(...this.lines.slice(-scale).filter((l) => l.line.state === 'active').map((l, i) => ({ id: `tm_${l.line.id}`, pid: 20000 + i, cwd: l.line.worktreePath ?? ROOT, cmd: [l.line.agent], title: l.local?.title ?? l.line.id, cols: 120, rows: 32, createdAt: l.line.lastActivityAt, exited: false, lineId: l.line.id, agent: l.line.agent })));
    this.detected = seedPorts(this.terms);
    this.tasks = [...seedTasks(now), ...scaledTasks(scale, now)];
    this.messages = seedMessages(this.tasks, now);
    this.activity = seedActivity();
    ({ workspaces: this.workspaces, folders: this.folders, groups: this.groups } = seedWs(now));
    this.live = opts.live ?? true;
    if (opts.switchboard ?? (typeof location !== 'undefined' && new URLSearchParams(location.search).has('switchboard')))
      this.switchboard = { found: true, dir: '/home/dev/.switchboard', imported: false, running: true, sessions: 12, tasks: 4, workspaces: 3, hooks: true, settings: { 'overseer.mockImported': '1' } };
    this.delayMs = opts.delayMs ?? 300;
    this.devicePlatform = opts.platform;
  }

  emit(type: EventType, data: unknown) {
    const e: DaemonEvent = { id: String(++this.seq), type, at: new Date().toISOString(), data };
    for (const s of this.subs) s(e);
  }

  async request<T>(method: string, path: string, body?: any): Promise<T> {
    await lag(`${method} ${path}`);
    return this.route(method, path, body) as T;
  }

  private switchboardRoute(method: string): unknown {
    const s = this.switchboard;
    if (method === 'GET') return structuredClone(s);
    if (method !== 'POST') throw new DaemonError(404, `no route ${method} /v1/import/switchboard`);
    if (!s.found) throw new DaemonError(404, 'no switchboard data found');
    this.switchboard = { ...s, imported: true, running: false };
    return { imported: true, sessions: s.sessions, tasks: s.tasks, workspaces: s.workspaces, stopped: s.running, hooks: s.hooks };
  }

  private route(method: string, fullPath: string, body: any): unknown {
    const [path, qs = ''] = fullPath.split('?');
    const m = (re: RegExp) => path.match(re);
    let r: RegExpMatchArray | null;
    if (path.startsWith('/v1/review')) return this.review.route(method, fullPath, body);
    if (method === 'GET' && path === '/v1/system') return mockSystem(qs);
    if (method === 'GET' && path === '/v1/usage') return mockUsage(qs);
    if (method === 'GET' && path === '/v1/fs/dirs') return mockDirs(new URLSearchParams(qs));
    if (method === 'GET' && path === '/v1/usage/statusline') return { ...this.statusline };
    if (method === 'POST' && (path === '/v1/usage/statusline/install' || path === '/v1/usage/statusline/uninstall')) {
      const on = path.endsWith('/install');
      const changed = this.statusline.installed !== on;
      this.statusline.installed = on;
      return { ...this.statusline, changed, backup: changed ? '~/.claude/settings.json.bak-202610300000' : null };
    }
    if (method === 'GET' && path === '/v1/version') return { version: '0.2.0', commit: 'mock', os: 'linux', arch: 'x86_64', protocol: 1 };
    if (method === 'GET' && path === '/v1/status') return { stationId: 'atlas', version: '0.2.0', activeCount: 7, idleCount: 3, heldCount: 2 };
    if (method === 'GET' && path === '/v1/lines') return structuredClone(this.lines);
    if (method === 'GET' && path === '/v1/terms') return structuredClone(this.terms);
    if (method === 'GET' && path === '/v1/tasks') return structuredClone(this.tasks);
    if (method === 'GET' && path === '/v1/projects') return structuredClone(this.projects);
    if (method === 'POST' && path === '/v1/projects') return this.createProject(body);
    if (method === 'POST' && path === '/v1/projects/discover') return this.discover();
    if (method === 'GET' && path === '/v1/projects/sources') return this.sourcesOf(new URLSearchParams(qs).get('project') ?? '');
    if (method === 'POST' && path === '/v1/projects/sources') return this.addSource(body);
    if (method === 'DELETE' && path.startsWith('/v1/projects/sources/')) return this.removeSource(decodeURIComponent(path.slice('/v1/projects/sources/'.length)));
    if (method === 'GET' && path === '/v1/projects/items') return this.sourceItems(new URLSearchParams(qs).get('project') ?? '');
    if (method === 'GET' && path === '/v1/agents') return structuredClone(this.agents);
    if (method === 'POST' && path === '/v1/agents/rescan') return structuredClone(this.agents);
    if (method === 'GET' && path === '/v1/hooks/status') return structuredClone(this.hooks);
    if (path === '/v1/import/switchboard') return this.switchboardRoute(method);
    if (path === '/v1/lifecycle' && (method === 'GET' || method === 'PATCH')) return this.lifecycleRoute(method, body);
    if (method === 'POST' && (path === '/v1/hooks/install' || path === '/v1/hooks/uninstall')) {
      const on = path.endsWith('/install');
      const want: string[] | undefined = body?.agents;
      const results: HookResult[] = [];
      for (const a of this.hooks.agents) {
        if (a.mechanism === 'polling' || (want && !want.includes(a.agent))) continue;
        const changed = a.installed !== on && (a.configured || !on);
        if (body?.dryRun !== false) {
          results.push({ agent: a.agent, path: a.path ?? '', changed, written: false, diff: changed ? `+ ${a.agent} hooks` : '' });
          continue;
        }
        if (changed) a.installed = a.current = on;
        results.push({ agent: a.agent, path: a.path ?? '', changed, written: changed, diff: '', ...(a.agent === 'codex' && changed && on ? { note: 'open codex and accept the hooks prompt' } : {}) });
      }
      return { dryRun: body?.dryRun !== false, results };
    }
    if (method === 'GET' && path === '/v1/profiles') return structuredClone(this.profileStore);
    if (method === 'PUT' && path === '/v1/profiles') return this.saveProfiles(body);
    if (method === 'GET' && path === '/v1/profiles/subagents') return { subagents: structuredClone(MOCK_SUBAGENTS) };
    if (method === 'POST' && path === '/v1/profiles/apply-files') return this.applyFiles(body);
    if (method === 'GET' && path === '/v1/env') return { path: '/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin', source: 'login-shell' };
    if (method === 'POST' && path === '/v1/uploads') return this.storeUpload(new URLSearchParams(qs).get('name') ?? '', 0);
    if (method === 'POST' && path === '/v1/terms') return this.createTerm(body);
    if (method === 'POST' && path === '/v1/feedback/start') return { term: this.createTerm({ agent: body.agent, cwd: body.cwd ?? '/tmp/feedback', title: 'Feedback' }) };
    if (method === 'GET' && path === '/v1/github/review-requests') return mockReviewRequests();
    if ((r = m(/^\/v1\/github\/review-requests\/([^/]+)\/([^/]+)\/(\d+)\/review$/)) && method === 'POST') {
      const project = this.projects[0]?.path ?? '/tmp/review';
      const term = this.createTerm({ agent: body.agent, cwd: project, worktree: { branch: `review/pr-${r[3]}` }, title: `PR #${r[3]} ${r[1]}/${r[2]}` });
      return { term, worktree: term.cwd, project, branch: `review/pr-${r[3]}` };
    }
    if (method === 'POST' && path === '/v1/feedback/direct') return { url: `https://github.com/${body.repo}/issues/1`, labelled: true };
    if (method === 'POST' && path === '/v1/tasks') return this.createTask(body);
    if ((r = m(/^\/v1\/lines\/([^/]+)\/chat\/agents\/([^/]+)$/))) return this.agentChat(decodeURIComponent(r[1]), decodeURIComponent(r[2]), new URLSearchParams(qs));
    if ((r = m(/^\/v1\/lines\/([^/]+)\/(chat|screen|keys|commands)$/))) return this.chatRoute(decodeURIComponent(r[1]), r[2], new URLSearchParams(qs), body);
    if (method === 'PUT' && (r = m(/^\/v1\/(lines|terms)\/([^/]+)\/workspace$/))) return this.assignWorkspace(r[1], decodeURIComponent(r[2]), body);
    if ((r = m(/^\/v1\/lines\/([^/]+)\/(activity|diff|hold|close|resume|pin|title)$/))) return this.lineAction(decodeURIComponent(r[1]), r[2], body, !fullPath.includes('dryRun=false'));
    if ((r = m(/^\/v1\/(workspaces|folders|groups)(?:\/([^/]+))?$/))) return this.wsRoute(method, r[1] as WsKind, r[2] && decodeURIComponent(r[2]), body);
    if (method === 'POST' && path === '/v1/broadcast') return this.broadcast(body);
    if (method === 'POST' && (r = m(/^\/v1\/ports\/([^/]+)\/kill$/))) return this.killPortProcess(decodeURIComponent(r[1]), body);
    if ((r = m(/^\/v1\/terms\/([^/]+)\/(resize|kill|title)$/))) return this.termAction(decodeURIComponent(r[1]), r[2], body);
    if (method === 'GET' && path === '/v1/tasks/auto-organize') return { enabled: this.autoOrganize };
    if (method === 'PUT' && path === '/v1/tasks/auto-organize') {
      this.autoOrganize = body?.enabled === true;
      return { enabled: this.autoOrganize };
    }
    if (method === 'POST' && path === '/v1/tasks/organize') return this.organize(body);
    if (path.startsWith('/v1/organize/')) return this.organizeRoute(method, path, body);
    if ((r = m(/^\/v1\/tasks\/([^/]+)\/messages$/))) return this.messagesRoute(method, decodeURIComponent(r[1]), body);
    if ((r = m(/^\/v1\/tasks\/([^/]+)(?:\/(start|review))?$/))) return this.taskAction(method, decodeURIComponent(r[1]), r[2], body);
    const g = this.git.route(method, path, new URLSearchParams(qs), body);
    if (g !== undefined) return g;
    throw new DaemonError(404, `no route ${method} ${path}`);
  }

  private chats = new Map<string, ChatEntry[]>();

  private chatOf(id: string): ChatEntry[] {
    let c = this.chats.get(id);
    if (!c) this.chats.set(id, (c = seedChat()));
    return c;
  }

  private agentChat(id: string, agentId: string, q: URLSearchParams): unknown {
    const l = this.findLine(id);
    const entries = seedAgentChat(agentId);
    if (!entries) throw new DaemonError(404, `no sub-agent ${agentId}`);
    return { ...chatPage(id, l.line.agent, entries, q), agentId };
  }

  private chatRoute(id: string, action: string, q: URLSearchParams, body: any): unknown {
    const l = this.findLine(id);
    if (action === 'chat') {
      if (!['claude', 'codex', 'gemini'].includes(l.line.agent)) throw new DaemonError(409, `no chat parser for ${l.line.agent}`);
      return chatPage(id, l.line.agent, this.chatOf(id), q);
    }
    if (action === 'screen') return mockScreen(id);
    if (action === 'keys') {
      const keys: string[] = Array.isArray(body?.keys) ? body.keys : [];
      if (l.local?.needsInput) {
        l.local = { ...l.local, needsInput: false, busy: true };
        this.emit('line.updated', { line: l.line, local: l.local });
      }
      return { sent: keys.length + (typeof body?.text === 'string' && body.text ? 1 : 0) };
    }
    return { commands: MOCK_COMMANDS };
  }

  private chatImage(id: string, q: URLSearchParams): ArrayBuffer {
    this.findLine(id);
    const num = (k: string) => (/^\d+$/.test(q.get(k) ?? '') ? Number(q.get(k)) : null);
    const at = num('at');
    const i = num('i');
    if (at === null || i === null) throw new DaemonError(400, 'at and i must be non-negative integers');
    const e = this.chatOf(id).find((x) => x.imagesAt === at);
    if (!e || i >= (e.images ?? 0)) throw new DaemonError(404, `no image ${i} at ${at}`);
    return Uint8Array.from(atob(MOCK_PNG), (ch) => ch.charCodeAt(0)).buffer;
  }

  private chatSent(target: string, text: string) {
    if (!target.startsWith('line:')) return;
    const id = target.slice(5);
    const c = this.chats.get(id);
    if (!c) return;
    const rev = c.reduce((m, e) => Math.max(m, e.rev), 0) + 1;
    c.push({ id: `u${rev}`, seq: c.length + 1, rev, kind: 'user', at: new Date().toISOString(), text });
    this.emit('line.chat', { lineId: id, cursor: rev });
  }

  private findLine(id: string) {
    const l = this.lines.find((x) => x.line.id === id);
    if (!l) throw new DaemonError(404, `unknown line ${id}`);
    return l;
  }

  private lineAction(id: string, action: string, body: any, dry = false): unknown {
    const l = this.findLine(id);
    if (dry && (action === 'hold' || action === 'close')) {
      const running = l.line.state !== 'held' && l.line.state !== 'hung-up';
      const plan = {
        lineId: id,
        action: action === 'hold' ? 'held' : 'closed',
        steps: running ? [`SIGTERM ${l.local?.pid ?? '?'}`] : [],
        predicates: { alive: running ? { ok: true } : { ok: false, detail: 'not running' }, atPrompt: l.local?.busy ? { ok: false, detail: 'turn in progress' } : { ok: true } },
      };
      if (!running || l.local?.busy) throw new DaemonError(409, 'refused', { ...plan, error: running ? 'turn in progress' : 'not running' });
      return plan;
    }
    switch (action) {
      case 'activity':
        if (!this.agents.find((a) => a.id === l.line.agent)?.sessions) throw new DaemonError(409, `no session files for ${l.line.agent}`);
        return structuredClone(this.activity[id]);
      case 'diff':
        if (l.line.repo === 'unknown') throw new DaemonError(409, 'fatal: not a git repository (or any of the parent directories): .git');
        return { lineId: id, worktree: l.line.worktreePath ?? '', status: ' M src/term.rs\n M tests/term.rs', stat: ' 2 files changed, 21 insertions(+), 3 deletions(-)', diff: bigDiff() ? bigPatch() : DIFF, truncated: false } satisfies Diff;
      case 'hold':
      case 'close': {
        const from = l.line.state;
        l.line.state = action === 'hold' ? 'held' : 'hung-up';
        l.local = { ...l.local, busy: false, needsInput: false, heldReason: action === 'hold' ? 'user' : undefined };
        this.emit(action === 'hold' ? 'line.state_changed' : 'line.hung_up', { line: l.line, from, to: l.line.state, local: l.local });
        return { lineId: id, action: action === 'hold' ? 'held' : 'closed', predicates: {} };
      }
      case 'pin':
        l.local = { ...l.local, pinned: !!body?.pinned };
        this.emit('line.updated', { line: l.line, local: l.local });
        return {};
      case 'title': {
        if (!body || !('title' in body) || (body.title !== null && typeof body.title !== 'string')) throw new DaemonError(400, 'title (string or null) required');
        const titleOverride = body.title === null ? null : cleanTitle(body.title);
        l.local = { ...l.local, titleOverride };
        this.emit('line.updated', { line: l.line, local: { titleOverride } });
        return { lineId: id, titleOverride };
      }
      case 'resume': {
        const existing = this.terms.find((t) => t.lineId === id && !t.exited);
        if (existing) return existing;
        if (body?.attachOnly) throw new DaemonError(409, 'the agent is not running: resume it to start it again');
        const from = l.line.state;
        l.line.state = 'active';
        l.local = { ...l.local, heldReason: undefined };
        this.emit('line.state_changed', { line: l.line, from, to: 'active', local: l.local });
        return this.spawn({ cwd: l.line.worktreePath ?? ROOT, cmd: [l.line.agent, '--resume', id], cols: 120, rows: 32, title: l.local?.title, lineId: id }, l.line.agent);
      }
    }
    return {};
  }

  private wsRoute(method: string, kind: WsKind, id: string | undefined, body: any): unknown {
    const list = this[kind] as (Workspace | Folder | Group)[];
    const single = kind.slice(0, -1) as 'workspace' | 'folder' | 'group';
    const now = new Date().toISOString();
    if (method === 'GET' && !id) return structuredClone(list).sort((a, b) => Number((b as Workspace).pinned ?? 0) - Number((a as Workspace).pinned ?? 0) || a.rank - b.rank);
    if (method === 'POST' && !id) {
      const name = String(body?.name ?? '').trim();
      let item: Workspace | Folder | Group;
      if (kind === 'workspaces') {
        const path = String(body?.path ?? '').trim().replace(/^~(?=\/|$)/, HOME).replace(/\/+$/, '');
        if (!path.startsWith('/')) throw new DaemonError(400, 'path must be absolute');
        const existing = this.workspaces.find((w) => w.path === path);
        if (existing) return structuredClone(existing);
        item = { id: `ws_${Math.random().toString(36).slice(2, 8)}`, name: name || path.split('/').pop() || path, path, station: 'atlas', folderId: body?.folderId ?? null, pinned: !!body?.pinned, archived: false, auto: false, rank: Date.now(), tabs: [], notifyIdle: false, createdAt: now, updatedAt: now };
      } else {
        if (!name) throw new DaemonError(400, 'name required');
        if (kind === 'folders') {
          const parent = body?.parentId ? this.folders.find((f) => f.id === body.parentId) : null;
          if (body?.parentId && (!parent || parent.parentId)) throw new DaemonError(400, 'parentId must be a top-level folder');
          item = { id: `f_${Math.random().toString(36).slice(2, 8)}`, name, parentId: parent?.id ?? null, rank: body?.rank ?? Date.now(), collapsed: false, createdAt: now, updatedAt: now };
        } else item = { id: `g_${Math.random().toString(36).slice(2, 8)}`, name, rank: Date.now(), members: body?.members ?? [], notifyIdle: false, createdAt: now, updatedAt: now };
      }
      list.push(item);
      this.emit(`${single}.created` as EventType, { [single]: item });
      return structuredClone(item);
    }
    const item = list.find((x) => x.id === id);
    if (!item) throw new DaemonError(404, `no ${single} ${id}`);
    if (method === 'DELETE') {
      (this as any)[kind] = list.filter((x) => x.id !== id);
      if (kind === 'folders') for (const w of this.workspaces) if (w.folderId === id) w.folderId = (item as Folder).parentId;
      if (kind === 'workspaces') this.unassign(item.id);
      this.emit(`${single}.deleted` as EventType, { id });
      return { deleted: id };
    }
    if (method === 'PATCH') {
      const { add, remove, id: _i, ...patch } = body ?? {};
      Object.assign(item, patch, { updatedAt: now });
      if (kind === 'groups') {
        const g = item as Group;
        g.members = [...new Set([...g.members.filter((k) => !(remove ?? []).includes(k)), ...(add ?? [])])];
      }
      this.emit(`${single}.updated` as EventType, { [single]: item });
      return structuredClone(item);
    }
    return structuredClone(item);
  }

  private targetsOf(b: BroadcastRequest): string[] {
    if (b.workspaceId) {
      const w = this.workspaces.find((x) => x.id === b.workspaceId);
      if (!w) throw new DaemonError(404, `no workspace ${b.workspaceId}`);
      const repo = MOCK_PROJECTS.find((p) => p.path === w.path)?.repo;
      return this.lines.filter((l) => l.line.repo === repo && l.line.state !== 'held' && l.line.state !== 'hung-up').map((l) => `line:${l.line.id}`);
    }
    if (b.groupId) {
      const g = this.groups.find((x) => x.id === b.groupId);
      if (!g) throw new DaemonError(404, `no group ${b.groupId}`);
      return g.members;
    }
    return [...(b.lineIds ?? []).map((id) => `line:${id}`), ...(b.termIds ?? []).map((id) => `term:${id}`)];
  }

  private busyReason(target: string): string | null {
    const id = target.slice(5);
    if (target.startsWith('term:')) {
      const t = this.terms.find((x) => x.id === id);
      return !t || t.exited ? 'gone' : null;
    }
    const l = this.lines.find((x) => x.line.id === id);
    if (!l || l.line.state === 'held' || l.line.state === 'hung-up') return 'gone';
    if (l.local?.needsInput) return 'needs input';
    if (l.local?.busy) return 'busy';
    return null;
  }

  private broadcast(b: BroadcastRequest): BroadcastResponse {
    const text = String(b?.text ?? '').trim();
    if (!text) throw new DaemonError(400, 'text required');
    const id = `bc_${Math.random().toString(36).slice(2, 8)}`;
    const onlyIdle = b.onlyIdle ?? true;
    const results: BroadcastResult[] = this.targetsOf(b).map((target) => {
      const why = this.busyReason(target);
      if (why === 'gone') return { target, result: 'refused', reason: 'not running' };
      if (!why || !onlyIdle) {
        this.delivered.push({ target, text });
        this.chatSent(target, text);
        return { target, result: 'sent' };
      }
      if (b.queue) {
        this.queued.push({ id, target, text, at: Date.now() });
        return { target, result: 'queued', reason: why };
      }
      return { target, result: 'skipped', reason: why };
    });
    return { id, results };
  }

  drainQueue() {
    const keep: typeof this.queued = [];
    for (const q of this.queued) {
      const why = this.busyReason(q.target);
      if (why === 'gone') this.emit('broadcast.delivered', { id: q.id, target: q.target, result: 'dropped', reason: 'not running' });
      else if (why) keep.push(q);
      else {
        this.delivered.push({ target: q.target, text: q.text });
        this.chatSent(q.target, q.text);
        this.emit('broadcast.delivered', { id: q.id, target: q.target, result: 'sent' });
      }
    }
    this.queued = keep;
  }

  private assignWorkspace(kind: string, id: string, body: any): unknown {
    const wid = body?.workspaceId;
    if (wid !== null && typeof wid !== 'string') throw new DaemonError(400, 'workspaceId must be a string or null');
    if (kind === 'lines') {
      const l = this.findLine(id);
      if (wid !== null && !this.workspaces.some((w) => w.id === wid)) throw new DaemonError(404, `no workspace ${wid}`);
      l.local = { ...l.local, workspaceId: wid };
      this.emit('line.updated', { line: l.line, local: l.local });
      return { lineId: id, workspaceId: wid };
    }
    const t = this.terms.find((x) => x.id === id);
    if (!t) throw new DaemonError(404, `unknown term ${id}`);
    if (wid !== null && !this.workspaces.some((w) => w.id === wid)) throw new DaemonError(404, `no workspace ${wid}`);
    if (wid) t.workspaceId = wid;
    else delete t.workspaceId;
    this.emit('term.updated', { term: t });
    return structuredClone(t);
  }

  private unassign(workspaceId: string) {
    for (const l of this.lines.filter((x) => x.local?.workspaceId === workspaceId)) {
      l.local = { ...l.local, workspaceId: null };
      this.emit('line.updated', { line: l.line, local: l.local });
    }
    for (const t of this.terms.filter((x) => x.workspaceId === workspaceId)) {
      delete t.workspaceId;
      this.emit('term.updated', { term: { ...t, workspaceId: null } });
    }
  }

  private termAction(id: string, action: string, body: any): unknown {
    const t = this.terms.find((x) => x.id === id);
    if (!t) throw new DaemonError(404, `unknown term ${id}`);
    if (action === 'resize') {
      Object.assign(t, { cols: body.cols, rows: body.rows });
      this.emit('term.updated', { term: t });
    }
    if (action === 'title') {
      if (!body || !('title' in body) || (body.title !== null && typeof body.title !== 'string')) throw new DaemonError(400, 'title (string or null) required');
      t.titleOverride = body.title === null ? null : cleanTitle(body.title);
      this.emit('term.updated', { term: t });
    }
    if (action === 'kill') {
      if (!body?.confirm) throw new DaemonError(400, 'confirm required');
      if (t.exited) throw new DaemonError(409, 'term already exited');
      t.exited = true;
      t.exitCode = 130;
      this.emit('term.exited', { term: t });
    }
    return structuredClone(t);
  }

  private createTerm(body: NewTerm): Term {
    const scopeCwd = body?.cwd;
    const project = body?.worktree && this.projects.find((p) => p.path === body.cwd);
    if (project) {
      const opts = typeof body.worktree === 'object' ? body.worktree : {};
      body = { ...body, cwd: this.git.createIn(project.path, { ...opts, agent: body.agent ?? 'shell' }).path };
    }
    if (body?.agent === undefined) return this.spawn(body, 'shell');
    if (body.cmd) throw new DaemonError(400, 'agent and cmd are mutually exclusive');
    const a = this.agents.find((x) => x.id === body.agent);
    if (!a) throw new DaemonError(400, `unknown agent ${body.agent}`);
    if (!a.installed) throw new DaemonError(400, `${a.name} is not installed`);
    if (body.profile && body.profile !== 'none' && !this.profileStore.profiles.some((p) => p.id === body.profile)) throw new DaemonError(400, `unknown profile ${body.profile}`);
    const p = a.id === 'shell' ? undefined : resolveProfile(this.profileStore, { explicit: body.profile, cwd: scopeCwd });
    const c = p?.agents[a.id] ?? {};
    const model = body.model || c.model || '';
    const effort = body.effort || c.effort;
    const cmd = a.id === 'shell' ? ['zsh'] : [a.id, ...(model ? ['--model', model] : []), ...(effort && a.efforts.includes(effort) ? ['--effort', effort] : [])];
    const profile = p || model || effort ? { ...(p ? { id: p.id, name: p.name } : {}), ...(model ? { model } : {}), ...(effort ? { effort } : {}) } : undefined;
    return this.spawn({ ...body, cmd, title: body.title ?? (a.id === 'shell' ? 'zsh' : a.name) }, a.id, profile);
  }

  private discover(): { added: number; projects: Project[] } {
    const fresh = DISCOVERABLE.filter((p) => !this.projects.some((x) => x.path === p.path));
    const now = new Date().toISOString();
    for (const p of fresh) {
      this.projects.push(structuredClone(p));
      if (this.workspaces.some((w) => w.path === p.path)) continue;
      const w: Workspace = { id: `ws_${p.name}`, name: p.name, path: p.path, station: 'atlas', folderId: null, pinned: false, archived: false, auto: true, rank: Date.now(), tabs: [], notifyIdle: false, createdAt: now, updatedAt: now };
      this.workspaces.push(w);
      this.emit('workspace.created', { workspace: w });
    }
    return { added: fresh.length, projects: structuredClone(this.projects) };
  }

  private lifecycle: Lifecycle = {
    afterReboot: 'agents',
    autostart: { platform: 'systemd', supported: true, installed: false, enabled: false, current: false, supervised: false, path: null, linger: false, lingerCommand: 'loginctl enable-linger alice' },
  };

  private lifecycleRoute(method: string, body: LifecyclePatch | undefined): Lifecycle {
    if (method === 'PATCH' && body) {
      if (body.afterReboot) {
        if (!['agents', 'shells', 'nothing'].includes(body.afterReboot)) throw new DaemonError(400, 'afterReboot must be agents, shells or nothing');
        this.lifecycle.afterReboot = body.afterReboot;
      }
      if (body.autostart !== undefined) {
        const on = body.autostart;
        this.lifecycle.autostart = { ...this.lifecycle.autostart, installed: on, enabled: on, current: on, path: on ? `${HOME}/.config/systemd/user/overseer-daemon.service` : null };
      }
      if (body.linger) this.lifecycle.autostart = { ...this.lifecycle.autostart, linger: true };
    }
    return structuredClone(this.lifecycle);
  }

  private uploaded = new Set<string>();

  private storeUpload(raw: string, size: number): UploadResult {
    const name = raw.split(/[\\/]/).pop()?.replace(/[^\w.-]/g, '_').replace(/^\.+/, '') || 'upload';
    const ext = /\.([A-Za-z0-9]{1,10})$/.exec(name)?.[1]?.toLowerCase();
    const file = `${Math.random().toString(36).slice(2, 8).padEnd(6, '0')}${ext ? `.${ext}` : ''}`;
    this.uploaded.add(`${HOME}/.overseer/u/${file}`);
    return { path: `${HOME}/.overseer/u/${file}`, short: `~/.overseer/u/${file}`, name, size };
  }

  private keepAttachments(id: string, list: unknown): string[] {
    if (list == null) return [];
    if (!Array.isArray(list)) throw new DaemonError(400, 'attachments must be a list');
    if (list.length > 20) throw new DaemonError(400, 'at most 20 attachments');
    const files = list.map((it) => {
      const raw = it?.path;
      if (typeof raw !== 'string') throw new DaemonError(400, 'attachment path required');
      const dir = `${HOME}/.overseer/u/`;
      if (!this.uploaded.has(raw)) throw new DaemonError(400, raw.startsWith(dir) ? `${raw}: no such upload` : `${raw} is not an upload`);
      const n = typeof it.name === 'string' ? it.name.trim() : '';
      const plain = n && !/[/\\\0]/.test(n) && n !== '.' && n !== '..';
      return { raw, name: plain ? n.replace(/[^A-Za-z0-9._-]/g, '_').replace(/_+/g, '_').replace(/^\.+/, '').slice(0, 80) || 'file' : raw.slice(dir.length) };
    });
    const kept: string[] = [];
    files.forEach(({ raw, name }, i) => {
      const to = `${HOME}/.overseer/tasks/${id}/${name}`;
      kept.push(kept.includes(to) ? `${HOME}/.overseer/tasks/${id}/${i + 1}-${name}` : to);
      this.uploaded.delete(raw);
    });
    return kept;
  }

  async uploadFile(path: string, onProgress?: (p: UploadProgress) => void): Promise<UploadResult> {
    if (path.endsWith('/')) throw new DaemonError(400, `${path}: folders can't be uploaded`);
    const total = 3 * 1024 * 1024;
    for (let i = 0; i <= 4; i++) {
      onProgress?.({ sent: (total * i) / 4, total });
      if (i < 4 && this.delayMs) await new Promise((r) => setTimeout(r, this.delayMs / 2));
    }
    return this.storeUpload(path, total);
  }

  async fetchBytes(path: string): Promise<ArrayBuffer> {
    const u = new URL(path, 'http://mock');
    const img = /^\/v1\/lines\/([^/]+)\/chat\/image$/.exec(u.pathname);
    if (img) return this.chatImage(decodeURIComponent(img[1]), u.searchParams);
    const file = u.searchParams.get('path') ?? '';
    if (u.pathname !== '/v1/file' || !file) throw new DaemonError(400, 'path required');
    if (!/^[~/]/.test(file) && !u.searchParams.get('term') && !u.searchParams.get('line')) throw new DaemonError(400, 'relative path needs a known term');
    if (this.delayMs) await new Promise((r) => setTimeout(r, this.delayMs));
    const kind = fileKind(file);
    if (kind === 'image' && file.toLowerCase().endsWith('.svg')) return utf8(mockSvg(file));
    if (kind === 'image') return Uint8Array.from(atob(MOCK_PNG), (ch) => ch.charCodeAt(0)).buffer;
    if (kind === 'markdown') return utf8(`# ${file.slice(file.lastIndexOf('/') + 1)}\n\nA **demo** file rendered in the app.\n\n- Images, video and audio\n- PDF and \`markdown\`\n\n\`\`\`ts\nconst ok = true;\n\`\`\`\n`);
    if (kind === 'text') return utf8(Array.from({ length: 40 }, (_, i) => `${String(i + 1).padStart(3)}  demo line for ${file}`).join('\n'));
    throw new DaemonError(404, 'not in demo');
  }

  async savePaste(name: string, bytes: Uint8Array): Promise<string> {
    return `/tmp/overseer-paste/${Date.now()}-${this.storeUpload(name, bytes.length).name}`;
  }

  async saveDownload(name: string, bytes: Uint8Array): Promise<string> {
    const url = URL.createObjectURL(new Blob([bytes as BlobPart]));
    const a = Object.assign(document.createElement('a'), { href: url, download: name });
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 0);
    return name;
  }

  private sourcesOf(project: string): TaskSource[] {
    if (!project.startsWith('/')) throw new DaemonError(400, 'project must be an absolute path');
    const repo = this.projects.find((p) => p.path === project)?.repo ?? '';
    const own = this.taskSources.filter((s) => s.project === project).map(({ project: _, ...s }) => s);
    const [, host, path] = /^([^/]+)\/(.+)$/.exec(repo) ?? [];
    const kind = host === 'github.com' ? 'github' : host?.includes('gitlab') ? 'gitlab' : null;
    const target = kind === 'gitlab' && host !== 'gitlab.com' ? `https://${repo}` : path;
    const implicit: TaskSource[] = kind && !own.some((s) => s.kind === kind && s.target === target) ? [{ id: kind, kind, target, implicit: true }] : [];
    return [...implicit, ...own];
  }

  private addSource(body: { project?: string; kind?: string; target?: string }): TaskSource {
    const project = String(body?.project ?? '');
    const target = String(body?.target ?? '').trim();
    if (!project.startsWith('/')) throw new DaemonError(400, 'project must be an absolute path');
    if (body?.kind === 'github' && !/^[A-Za-z0-9][\w.-]*\/[A-Za-z0-9][\w.-]*$/.test(target)) throw new DaemonError(400, 'GitHub source must look like owner/repo');
    if (body?.kind === 'gitlab' && !/^(https:\/\/[\w.-]+\/)?[A-Za-z0-9][\w.-]*(\/[A-Za-z0-9][\w.-]*)+$/.test(target)) throw new DaemonError(400, 'GitLab source must look like group/repo or https://host/group/repo');
    if (body?.kind === 'jira' && !target) throw new DaemonError(400, 'Jira source needs a JQL query');
    if (body?.kind !== 'github' && body?.kind !== 'gitlab' && body?.kind !== 'jira') throw new DaemonError(400, 'kind must be one of github, gitlab, jira');
    if (this.taskSources.some((s) => s.project === project && s.kind === body.kind && s.target === target)) throw new DaemonError(409, 'this source is already added');
    const src: TaskSource = { id: `src_${Math.random().toString(36).slice(2, 8)}`, kind: body.kind, target, implicit: false };
    this.taskSources.push({ ...src, project });
    return structuredClone(src);
  }

  private removeSource(id: string) {
    const i = this.taskSources.findIndex((s) => s.id === id);
    if (i < 0) throw new DaemonError(404, `no source ${id}`);
    this.taskSources.splice(i, 1);
    return { ok: true };
  }

  private sourceItems(project: string): SourceItems {
    const sources = this.sourcesOf(project);
    const items: SourceItem[] = [];
    const errors: SourceItems['errors'] = [];
    for (const s of sources) {
      if (s.target.includes('fail')) {
        errors.push({ source: s.id, error: `${{ github: 'gh', gitlab: 'glab', jira: 'jira' }[s.kind]} issue: could not resolve ${s.target}` });
        continue;
      }
      if (s.kind === 'github') {
        for (const [n, title] of [
          [66, 'Idea: Run command with ! like claude code'],
          [63, "Bug: Doesn't reduce text correctly"],
          [50, 'Idea: add an option to add tasks to a project from multi sources'],
        ] as const) {
          items.push({ source: s.id, kind: 'github', key: `#${n}`, title, body: `Reported from Overseer.\n\nSee ${s.target}#${n}.`, url: `https://github.com/${s.target}/issues/${n}` });
        }
      } else if (s.kind === 'gitlab') {
        items.push({ source: s.id, kind: 'gitlab', key: '#12', title: 'Pipeline cache misses on main', body: 'Cache key changes every run.', url: `https://gitlab.com/${s.target}/-/issues/12` });
      } else {
        items.push({ source: s.id, kind: 'jira', key: 'PROJ-1204', title: 'Export orders as CSV', body: '', url: '', ticketKey: 'PROJ-1204' });
        items.push({ source: s.id, kind: 'jira', key: 'OPS-7', title: 'Rotate staging certificates', body: '', url: '', ticketKey: null });
      }
    }
    return { sources, items, errors };
  }

  private createProject(body: { path?: string }): Project {
    const raw = String(body?.path ?? '').trim().replace(/^~(?=\/|$)/, '/home/alice').replace(/\/+$/, '');
    if (!raw.startsWith('/')) throw new DaemonError(400, `${body?.path ?? ''}: not a directory`);
    const known = this.projects.find((p) => raw === p.path || raw.startsWith(`${p.path}/`));
    if (known) return structuredClone(known);
    if (/not-a-repo|tmp/.test(raw)) throw new DaemonError(400, `${raw}: not a git repository`);
    const p: Project = { path: raw, name: raw.split('/').pop() || raw, repo: 'unknown' };
    this.projects.push(p);
    return structuredClone(p);
  }

  private spawn(body: NewTerm, agent: string, profile?: ProfileChip): Term {
    const t: Term = {
      id: `tm_${Math.random().toString(36).slice(2, 8)}`,
      pid: 6000 + this.terms.length,
      cwd: body.cwd ?? '/home/alice',
      cmd: body.cmd?.length ? body.cmd : ['zsh'],
      title: body.title ?? body.cmd?.[0] ?? 'zsh',
      agent,
      cols: body.cols ?? 80,
      rows: body.rows ?? 24,
      createdAt: new Date().toISOString(),
      exited: false,
      lineId: body.lineId ?? null,
      taskId: body.taskId ?? null,
      ...(profile ? { profile } : {}),
    };
    this.terms.push(t);
    this.emit('term.created', { term: t });
    return structuredClone(t);
  }

  private saveProfiles(body: Partial<ProfilesStore>): ProfilesStore {
    const next = structuredClone(this.profileStore);
    if (body?.default !== undefined) next.default = body.default;
    if (body?.scopes) next.scopes = body.scopes;
    if (body?.profiles) next.profiles = [...next.profiles.filter((p) => p.builtin), ...body.profiles.filter((p: Profile) => !p.builtin)];
    const ids = next.profiles.map((p) => p.id);
    if (next.default && next.default !== 'none' && !ids.includes(next.default)) throw new DaemonError(400, `unknown default profile ${next.default}`);
    this.profileStore = next;
    this.emit('profiles.changed', {});
    return structuredClone(next);
  }

  private applyFiles(body: { profile: string; dryRun?: boolean; files?: string[] }) {
    const p = this.profileStore.profiles.find((x) => x.id === body?.profile);
    if (!p) throw new DaemonError(400, `unknown profile ${body?.profile}`);
    const subs = p.agents.claude?.subagents ?? {};
    const files = MOCK_SUBAGENTS.filter((d) => subs[d.name])
      .map((d) => ({
        path: d.path,
        name: d.name,
        scope: d.scope,
        changes: (['model', 'effort'] as const).filter((k) => subs[d.name][k] && subs[d.name][k] !== d[k]).map((k) => ({ key: k, from: d[k], to: subs[d.name][k]! })),
      }))
      .filter((f) => f.changes.length);
    if (body.dryRun !== false) return { dryRun: true, files };
    if (!Array.isArray(body.files)) throw new DaemonError(400, 'files must list the confirmed paths');
    return { dryRun: false, files: files.filter((f) => body.files!.includes(f.path)).map((f) => ({ ...f, written: true, backup: `${f.path}.bak-202610291200` })) };
  }

  private createTask(body: NewTask): Task {
    if (!body?.title?.trim()) throw new DaemonError(400, 'title required');
    const now = new Date().toISOString();
    const id = `tk_${Math.random().toString(36).slice(2, 8)}`;
    const kept = this.keepAttachments(id, body.attachments);
    const head = (body.description ?? '').trimEnd();
    const task: Task = {
      id,
      number: Math.max(100, ...this.tasks.map((t) => t.number)) + 1,
      title: body.title.trim(),
      description: kept.length ? `${head}${head ? '\n\n' : ''}Attachments:\n${kept.map((k) => `- ${k}`).join('\n')}` : (body.description ?? ''),
      project: body.project,
      status: body.status ?? 'pending',
      agent: body.agent,
      model: body.model,
      effort: body.effort ?? (resolveProfile(this.profileStore, { explicit: body.profile, cwd: body.project })?.agents[body.agent]?.effort ? ('' as Effort) : 'high'),
      ...(body.profile ? { profile: body.profile } : {}),
      rank: Math.max(0, ...this.tasks.filter((x) => x.status === (body.status ?? 'pending')).map((x) => x.rank)) + 1,
      station: 'atlas',
      ticketKey: body.ticketKey || null,
      ...(typeof (body as any).urgent === 'boolean' ? { urgent: (body as any).urgent } : {}),
      ...(typeof (body as any).important === 'boolean' ? { important: (body as any).important } : {}),
      ...((body as any).origin ? { origin: (body as any).origin } : {}),
      ...((body as any).parentId ? { parentId: (body as any).parentId } : {}),
      dependsOn: body.dependsOn ?? [],
      ...(typeof body.inheritBranch === 'boolean' ? { inheritBranch: body.inheritBranch } : {}),
      createdAt: now,
      updatedAt: now,
    };
    this.tasks.push(task);
    this.emit('task.created', { task });
    return structuredClone(task);
  }

  private messagesRoute(method: string, id: string, body: any): unknown {
    const t = this.findTask(id);
    if (method === 'GET') return structuredClone(this.messages.filter((m) => m.taskId === id || m.from === id));
    const text = String(body?.body ?? '').trim();
    if (!text || text.length > 8000) throw new DaemonError(400, 'body must be 1 to 8000 characters');
    const sender = body?.from ? this.findTask(body.from) : null;
    const live = t.status === 'in_progress' && !!t.termId;
    const m: TaskMessage = {
      id: `msg_${Math.random().toString(36).slice(2, 8)}`,
      taskId: id,
      from: sender?.id ?? null,
      ...(sender ? { fromNumber: sender.number, fromTitle: sender.title } : {}),
      body: text,
      createdAt: new Date().toISOString(),
      delivered: live,
    };
    this.messages.push(m);
    this.emit('task.message', { message: m });
    return structuredClone(m);
  }

  private organize(body: any): unknown {
    const projects = body?.project ? [body.project] : [...new Set(this.tasks.filter((t) => t.status !== 'completed' && !t.origin?.match(/organizer|session/)).map((t) => t.project))];
    if (!projects.length) throw new DaemonError(409, 'nothing to organize');
    const started: Task[] = [];
    for (const project of projects) {
      if (this.tasks.some((t) => t.origin === 'organizer' && t.project === project && t.status === 'in_progress')) continue;
      const org = this.createTask({ title: 'Organize board', description: '', project, agent: 'claude', model: '', status: 'in_progress' });
      const task = this.findTask(org.id);
      task.origin = 'organizer';
      task.progress = { done: 1, total: 3, note: 'Classifying tasks' };
      started.push(this.touch(task));
      setTimeout(() => {
        for (const t of this.tasks) if (t.project === project && t.origin !== 'organizer' && t.origin !== 'session' && t.status !== 'completed' && typeof t.urgent !== 'boolean') {
          t.urgent = t.important = false;
          this.touch(t);
        }
        task.status = 'completed';
        this.touch(task);
      }, 2500);
    }
    return { started };
  }

  proposals: Proposal[] = [];

  private plan(source: Proposal['source'], taskId?: string): Proposal {
    const now = Date.now();
    const scattered = this.lines.filter((l) => l.line.worktreePath?.startsWith(SCRATCH));
    const actions: Action[] = [{ id: 'a1', kind: 'create_folder', name: 'Scratch', ref: 'f1' }];
    const reasons: Record<string, string> = { a1: 'Several projects live under /srv/scratch with no workspace' };
    for (const repo of [...new Set(scattered.map((l) => l.line.worktreePath!.split('/').slice(0, 4).join('/')))].slice(0, 3)) {
      const id = `a${actions.length + 1}`;
      actions.push({ id, kind: 'create_workspace', path: repo, folder: 'f1' });
      reasons[id] = `${scattered.filter((l) => l.line.worktreePath!.startsWith(repo)).length} sessions in ${repo} belong to no workspace`;
    }
    const push = (a: Omit<Action, 'id'>, why: string) => {
      const id = `a${actions.length + 1}`;
      actions.push({ id, ...a } as Action);
      reasons[id] = why;
    };
    push({ kind: 'move_workspace', workspaceId: 'ws_thesis', folder: 'f_uni' }, 'The thesis is school work, next to compilers and os-labs');
    push({ kind: 'create_group', name: 'PROJ-108xx', lines: ['ln_01', 'ln_02', 'ln_05', 'ln_08'] }, 'Four webshop sessions on PROJ-108xx tickets');
    push({ kind: 'pin', lineId: 'ln_04' }, 'Busy most of the week');
    push({ kind: 'archive_workspace', workspaceId: 'ws_thesis' }, 'Its folder is gone and nothing ran there for 30 days');
    push({ kind: 'hold', lineId: 'ln_10' }, 'Idle for 34 days, holding frees its memory; resume brings it back');
    push({ kind: 'hold', lineId: 'ln_09' }, 'Idle for 31 days');
    push({ kind: 'close', lineId: 'ln_12' }, 'Its worktree folder was deleted');
    push({ kind: 'set_task_status', taskId: 'tk_09', status: 'completed' }, 'Its branch landed in main');
    push({ kind: 'track', lineId: 'ln_07' }, 'Older than the session task window, still has open work');
    const p: Proposal = {
      id: `pr_${now.toString(36)}`,
      version: 1,
      createdAt: new Date(now).toISOString(),
      status: 'pending_review',
      source,
      ...(taskId ? { taskId } : {}),
      summary: source === 'agent' ? 'Grouped the ticket work, filed the scratch repos and parked what has been idle for a month.' : `${this.lines.length} sessions: ${scattered.length} outside any workspace, 2 idle for over 30 days, 1 with a missing folder.`,
      actions,
      pickUp: [
        { lineId: 'ln_03', summary: 'Debounce terminal resize events: 4 files changed, tests pending', why: 'dirty', branch: 'feat/term-resize', edited: 4 },
        { lineId: 'ln_07', summary: 'Install Talos on the node2 node and join the cluster', why: 'todos', branch: 'feat/talos-node2', todos: 3 },
        { lineId: 'ln_09', summary: 'Proofread the remote mode chapter', why: 'recent', branch: 'docs/user-guide' },
      ],
      reasons,
    };
    this.proposals.unshift(p);
    this.emit('organize.proposal', { proposal: structuredClone(p) });
    return p;
  }

  private applyPlan(p: Proposal, ids: string[]): Result[] {
    const refs = new Map<string, string>();
    const results: Result[] = [];
    for (const a of p.actions.filter((x) => ids.includes(x.id))) {
      const prev = p.results?.find((r) => r.actionId === a.id && r.status !== 'failed');
      const r: Result = { actionId: a.id, kind: a.kind, status: prev ? 'skipped' : 'done' };
      if (!prev) {
        try {
          if (a.kind === 'create_folder') refs.set(a.ref!, (this.wsRoute('POST', 'folders', undefined, { name: a.name }) as Folder).id);
          if (a.kind === 'create_workspace') this.wsRoute('POST', 'workspaces', undefined, { path: a.path, folderId: refs.get(a.folder!) ?? a.folder ?? null });
          if (a.kind === 'move_workspace') this.wsRoute('PATCH', 'workspaces', a.workspaceId, { folderId: refs.get(a.folder!) ?? a.folder });
          if (a.kind === 'archive_workspace') this.wsRoute('PATCH', 'workspaces', a.workspaceId, { archived: true });
          if (a.kind === 'create_group') this.wsRoute('POST', 'groups', undefined, { name: a.name, members: a.lines!.map((l) => `line:${l}`) });
          if (a.kind === 'pin') this.lineAction(a.lineId!, 'pin', { pinned: true });
          if (a.kind === 'hold' || a.kind === 'close') {
            const l = this.findLine(a.lineId!);
            if ((a.kind === 'hold' && l.line.state === 'held') || (a.kind === 'close' && l.line.state === 'hung-up')) r.status = 'skipped';
            else if (l.local?.busy) throw new DaemonError(409, 'a turn is in progress');
            else this.lineAction(a.lineId!, a.kind, {});
          }
          if (a.kind === 'set_task_status') this.taskAction('PATCH', a.taskId!, undefined, { status: a.status });
          if (a.kind === 'track' && this.tasks.some((x) => x.lineId === a.lineId)) r.status = 'skipped';
          else if (a.kind === 'track') {
            const l = this.findLine(a.lineId!);
            const made = this.findTask(this.createTask({ title: l.local?.title ?? l.line.branch ?? l.line.id, description: '', project: l.line.worktreePath ?? HOME, agent: l.line.agent, model: '' }).id);
            made.origin = 'session';
            made.lineId = l.line.id;
            this.touch(made);
          }
        } catch (e) {
          r.status = 'failed';
          r.detail = String((e as Error).message ?? e);
        }
      }
      results.push(r);
    }
    p.results = [...(p.results ?? []).filter((x) => !results.some((r) => r.actionId === x.actionId)), ...results];
    p.status = 'applied';
    return results;
  }

  private organizeRoute(method: string, path: string, body: any): unknown {
    let r: RegExpMatchArray | null;
    if (method === 'POST' && path === '/v1/organize/deterministic') return structuredClone(this.plan('deterministic'));
    if (method === 'GET' && path === '/v1/organize/proposals') return structuredClone(this.proposals);
    if (method === 'POST' && path === '/v1/organize/start') {
      const agent = String(body?.agent ?? 'claude');
      const made = this.createTask({ title: 'Organize sessions', description: '', project: HOME, agent, model: body?.model ?? '', status: 'in_progress' });
      const task = this.findTask(made.id);
      task.origin = 'configurator';
      this.touch(task);
      if (this.live)
        setTimeout(() => {
          task.status = 'completed';
          this.touch(task);
          this.plan('agent', task.id);
        }, 2500);
      return { task: structuredClone(task), mcp: agent === 'claude' ? 'configured' : 'manual' };
    }
    if ((r = path.match(/^\/v1\/organize\/proposals\/([^/]+)(\/apply)?$/))) {
      const p = this.proposals.find((x) => x.id === decodeURIComponent(r![1]));
      if (!p) throw new DaemonError(404, `no proposal ${r[1]}`);
      if (method === 'GET' && !r[2]) return structuredClone(p);
      if (method === 'DELETE' && !r[2]) {
        p.status = 'dismissed';
        return { id: p.id, status: p.status };
      }
      if (method === 'POST' && r[2]) {
        const ids: string[] = Array.isArray(body?.actionIds) ? body.actionIds : [];
        const results = this.applyPlan(p, ids);
        this.emit('organize.proposal', { proposal: structuredClone(p) });
        return { proposalId: p.id, results };
      }
    }
    throw new DaemonError(404, `no route ${method} ${path}`);
  }

  private findTask(id: string) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) throw new DaemonError(404, `unknown task ${id}`);
    return t;
  }

  private touch(t: Task) {
    t.updatedAt = new Date().toISOString();
    this.emit('task.updated', { task: t });
    return structuredClone(t);
  }

  private taskAction(method: string, id: string, action: string | undefined, body: any): unknown {
    const t = this.findTask(id);
    if (!action && method === 'DELETE') {
      this.tasks = this.tasks.filter((x) => x.id !== id);
      this.emit('task.deleted', { id });
      for (const x of this.tasks) if (x.dependsOn?.includes(id)) {
        x.dependsOn = x.dependsOn.filter((d) => d !== id);
        this.touch(x);
      }
      return {};
    }
    if (!action && method === 'PATCH') {
      const { id: _id, number: _n, ...patch } = body ?? {};
      if (Array.isArray(patch.dependsOn) && patch.dependsOn.includes(id)) throw new DaemonError(400, 'a task cannot depend on itself');
      Object.assign(t, patch);
      if (t.origin === 'session' && patch.status) t.manualStatus = patch.status;
      else if (t.status === 'in_progress' && !t.termId) this.startTask(t);
      return this.touch(t);
    }
    if (action === 'start') {
      if (!body?.inPlace && !t.branch && this.projects.some((p) => p.path === t.project)) {
        const slug = t.title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 40);
        const w = this.git.createIn(t.project, { branch: `task/${t.number}-${slug}` });
        t.branch = w.branch;
        t.worktreePath = w.path;
      }
      this.startTask(t);
      return this.touch(t);
    }
    if (action === 'review') {
      if (body?.action === 'approve' && (body.merge === true || body.approvePr === true) && !t.termId) throw new DaemonError(409, 'the agent is not running; start the task again');
      t.status = body?.action === 'reject' ? 'in_progress' : 'completed';
      if (body?.action === 'reject') t.progress = { done: 1, total: 5, note: 'Addressing review' };
      const task = this.touch(t);
      return body?.action === 'commit' ? { task, commit: 'a1b2c3d' } : { task };
    }
    return structuredClone(t);
  }

  private startTask(t: Task) {
    t.status = 'in_progress';
    t.progress = { done: 0, total: 5, note: 'Starting' };
    const term = this.spawn({ cwd: t.worktreePath ?? t.project, cmd: [t.agent, ...(t.model ? ['--model', t.model] : [])], cols: 120, rows: 32, title: t.title, taskId: t.id }, t.agent);
    t.termId = term.id;
  }

  subscribe(onEvent: (e: DaemonEvent) => void): () => void {
    this.subs.add(onEvent);
    if (this.live && this.timers.length === 0) {
      this.timers.push(setInterval(() => this.tick(), 2500));
      if (this.storm > 0) this.timers.push(setInterval(() => this.stormTick(), 1000 / this.storm));
    }
    return () => {
      this.subs.delete(onEvent);
      if (this.subs.size === 0) this.stop();
    };
  }

  stop() {
    this.review.stop();
    for (const t of this.timers) clearInterval(t);
    this.timers = [];
  }

  private storm = 0;
  private stormSeq = 0;

  stormTick() {
    const n = ++this.stormSeq;
    const l = this.lines[n % this.lines.length];
    switch (n % 4) {
      case 0:
        l.line.lastActivityAt = new Date().toISOString();
        this.emit('line.activity', { lineId: l.line.id });
        break;
      case 1: {
        const t = this.terms[n % this.terms.length];
        this.emit('term.updated', { term: { ...t, title: `${t.title.replace(/ #\d+$/, '')} #${n}` } });
        break;
      }
      case 2: {
        const t = this.tasks[n % this.tasks.length];
        t.updatedAt = new Date().toISOString();
        this.emit('task.updated', { task: structuredClone(t) });
        break;
      }
      default:
        this.emit('line.updated', { line: { ...l.line, lastActivityAt: new Date().toISOString() }, local: l.local });
    }
  }

  tick() {
    this.drainQueue();
    const running = this.tasks.filter((t) => t.status === 'in_progress');
    const t = running[this.seq % Math.max(1, running.length)];
    if (t?.progress) {
      t.progress = { ...t.progress, done: t.progress.done + 1, note: ['Reading code', 'Implementing', 'Writing tests', 'Running tests', 'Polishing'][t.progress.done % 5] };
      if (t.progress.done >= t.progress.total) {
        t.status = 'in_testing';
        t.progress = { done: t.progress.total, total: t.progress.total };
      }
      this.touch(t);
    }
    const busy = this.lines.filter((l) => l.local?.busy && this.activity[l.line.id]);
    const l = busy[this.seq % Math.max(1, busy.length)];
    if (l) {
      const a = this.activity[l.line.id];
      const steps = a.workLog.map((s) => (s.state === 'running' ? { ...s, state: 'done' as const } : s));
      const cmds = ['Read src/router.rs', 'Edit src/router.rs', 'cargo test router', 'Edit tests/router.rs', 'cargo clippy'];
      steps.push({ kind: 'bash', label: cmds[this.seq % cmds.length], state: 'running', at: new Date().toISOString() });
      this.activity[l.line.id] = { ...a, workLog: steps.slice(-12), status: steps[steps.length - 1].label };
      l.line.lastActivityAt = new Date().toISOString();
      this.emit('line.activity', { lineId: l.line.id });
    }
  }

  async attachTerm(termId: string, onData: (bytes: Uint8Array) => void, onClose?: () => void): Promise<TermIO> {
    await lag(`ATTACH ${termId}`);
    const term = this.terms.find((t) => t.id === termId);
    if (!term) throw new DaemonError(404, `unknown term ${termId}`);
    const out = (s: string) => onData(enc.encode(s));
    this.sinks.set(termId, out);
    const agent = term.agent && term.agent !== 'shell' ? term.agent : null;
    const prompt = agent ? '\r\n\x1b[1m>\x1b[0m ' : `\r\n\x1b[32m${term.cwd}\x1b[0m \x1b[2m$\x1b[0m `;
    if (agent) {
      const act = term.lineId ? this.activity[term.lineId] : undefined;
      out(`\x1b[1m${agentInfo(agent)?.name ?? agent}\x1b[0m  \x1b[2m${term.cwd}\x1b[0m\r\n`);
      if (act) {
        for (const s of act.workLog) out(`\r\n\x1b[${s.state === 'running' ? '34' : '2'}m●\x1b[0m ${s.label}`);
        out(`\r\n\r\n${act.lastAssistantText}\r\n`);
      }
    } else {
      out(`\x1b[2mLast login: ${new Date(term.createdAt).toUTCString()} on ttys001\x1b[0m\r\n`);
    }
    out(prompt);
    let buf = '';
    let closed = false;
    return {
      write: (data) => {
        if (closed) return;
        for (const ch of data) {
          if (ch === '\r') {
            const cmd = buf.trim();
            buf = '';
            if (cmd === 'exit') {
              out('\r\n');
              closed = true;
              onClose?.();
              return;
            }
            out('\r\n' + fakeRun(cmd, agent, term.cwd));
            out(prompt);
          } else if (ch === '\x7f') {
            if (buf) {
              buf = buf.slice(0, -1);
              out('\b \b');
            }
          } else if (ch >= ' ') {
            buf += ch;
            out(ch);
          }
        }
      },
      resize: (cols, rows) => {
        const by = typeof location !== 'undefined' && new URLSearchParams(location.search).has('phone') ? 'phone' : 'desktop';
        if (term.cols === cols && term.rows === rows && term.sizedBy === by) return;
        Object.assign(term, { cols, rows, sizedBy: by });
        this.emit('term.updated', { term: { ...term } });
      },
      close: () => {
        closed = true;
        if (this.sinks.get(termId) === out) this.sinks.delete(termId);
      },
    };
  }

  feed(termId: string, text: string): boolean {
    const sink = this.sinks.get(termId);
    sink?.(text);
    return !!sink;
  }

  private onConn: ((s: ConnState) => void) | null = null;

  async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    this.onConn = onState;
    this.target = target;
    this.forwards = [];
    this.unforwarded.clear();
    this.detected = seedPorts(this.terms);
    const saved = target.kind === 'direct' ? this.hosts.find((h) => h.id === target.host) : undefined;
    const resolved =
      target.kind === 'ssh'
        ? `${target.host} → ${target.host}.internal:22 as alice`
        : saved
          ? `${saved.user}@${saved.host}:${saved.port}${saved.jump ? ` via ProxyJump ${saved.jump.user}@${saved.jump.host}:${saved.jump.port}` : ''}`
          : undefined;
    const phases: ConnPhase[] =
      target.kind === 'ssh' ? ['connecting', 'checking', 'installing', 'starting', 'connected'] : target.kind === 'direct' ? ['connecting', 'checking', 'starting', 'connected'] : ['connecting', 'connected'];
    const server = (saved?.host ?? target.host) === 'old-server' ? { running: '0.1.0', installed: '0.2.0', terms: 2, reason: 'hot upgrade: not found' } : undefined;
    for (const phase of phases) {
      onState({ phase, target, resolved, version: server && phase === 'connected' ? server.running : '0.2.0', detail: phase === 'installing' ? 'linux-x86_64 (8.1 MB)' : undefined, server: phase === 'connected' ? server : undefined });
      if (phase === 'connecting' && target.kind === 'ssh') await this.mockPrompts(target, resolved, onState);
      if (phase === 'connecting' && target.kind === 'direct') await this.mockDirect(target, saved, resolved, onState);
      if (phase !== 'connected' && this.delayMs) await new Promise((r) => setTimeout(r, this.delayMs));
      if (target.host === 'unreachable') {
        onState({
          phase: 'error',
          target,
          resolved,
          error: 'ssh: connect to host unreachable port 22: Connection timed out\nkex_exchange_identification: read: Connection reset by peer',
        });
        throw new Error('Connection timed out');
      }
    }
  }

  private async mockPrompts(target: HostTarget, resolved: string | undefined, onState: (s: ConnState) => void) {
    const host = target.host ?? '';
    const ask = (prompt: Omit<NonNullable<ConnState['prompt']>, 'id'>) => {
      const id = ++this.promptSeq;
      onState({ phase: 'connecting', target, resolved, prompt: { id, ...prompt } });
      return new Promise<string | null>((resolve) => this.prompts.set(id, resolve));
    };
    const fail = (error: string) => {
      onState({ phase: 'error', target, resolved, error });
      throw new Error(error);
    };
    if (host.includes('hostkey')) {
      const fingerprint = 'SHA256:p2QAMXNIC1TJYWeIOttrVc98/R1BUFWu3/LiyKgUfQM';
      const answer = await ask({
        kind: 'hostkey',
        host: `${host}.internal (10.0.4.21)`,
        fingerprint,
        text: `The authenticity of host '${host}.internal (10.0.4.21)' can't be established.\nED25519 key fingerprint is ${fingerprint}.\nThis key is not known by any other names.\nAre you sure you want to continue connecting (yes/no/[fingerprint])?`,
      });
      if (answer !== 'yes') fail('Host key verification failed.');
      onState({ phase: 'connecting', target, resolved });
    }
    if (host.includes('secret')) {
      const answer = await ask({ kind: 'secret', text: "Enter passphrase for key '/home/alice/.ssh/id_ed25519':" });
      if (answer === null) fail('alice@' + host + ': Permission denied (publickey).');
      onState({ phase: 'connecting', target, resolved });
    }
  }

  private async mockDirect(target: HostTarget, saved: SavedHost | undefined, resolved: string | undefined, onState: (s: ConnState) => void) {
    const fail = (error: string) => {
      onState({ phase: 'error', target, resolved, error });
      throw new Error(error);
    };
    if (!saved) fail(`no saved host ${target.host}`);
    const host = saved!;
    if (!this.trusted.has(host.id)) {
      const fingerprint = 'SHA256:Jm0d3vN1b8KQ2xXo3c1Q9cB5n0yq8l7s9WbT4f2pE6k';
      const id = ++this.promptSeq;
      const name = host.port === 22 ? host.host : `[${host.host}]:${host.port}`;
      onState({ phase: 'connecting', target, resolved, prompt: { id, kind: 'hostkey', host: name, fingerprint, text: `The authenticity of host '${name}' can't be established.\nKey fingerprint is ${fingerprint}.\nTrust it and connect?` } });
      const answer = await new Promise<string | null>((resolve) => this.prompts.set(id, resolve));
      if (answer !== 'yes') fail(`Host key of ${host.host} not trusted (cancelled)\n(${resolved})`);
      this.trusted.add(host.id);
      onState({ phase: 'connecting', target, resolved });
    }
    if (host.auth === 'password') {
      const id = ++this.promptSeq;
      onState({ phase: 'connecting', target, resolved, prompt: { id, kind: 'secret', text: `Password for ${host.user}@${host.host}:${host.port}:` } });
      const answer = await new Promise<string | null>((resolve) => this.prompts.set(id, resolve));
      if (answer === null) fail(`password for ${host.user}@${host.host}:${host.port}: cancelled`);
      onState({ phase: 'connecting', target, resolved });
    }
  }

  async platform(): Promise<PlatformInfo> {
    if (this.devicePlatform) return { ...this.devicePlatform };
    const phone = typeof location !== 'undefined' && phoneParam(location.search);
    return { os: phone ? 'android' : 'linux', mobile: phone };
  }

  async wake(): Promise<void> {}

  async restartServer(): Promise<void> {
    const target = this.target;
    if (!target) return;
    this.onConn?.({ phase: 'reconnecting', target, attempt: 1, detail: 'restarting the server' });
    if (this.delayMs) await new Promise((r) => setTimeout(r, this.delayMs));
    this.onConn?.({ phase: 'connected', target, version: '0.2.0' });
  }

  async deviceKey(): Promise<DeviceKey> {
    const publicKey = 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIK9mZ3Jm0d3vN1b8KQ2xXo3c1Q9cB5n0yq8l7s9WbT4f overseer-android';
    return {
      publicKey,
      fingerprint: 'SHA256:4n2bq0Y8m3HkqJcV9wZ1sX7pT6rLdE5uA0fG2hN8iKo',
      installCommand: `mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && (grep -qxF '${publicKey}' ~/.ssh/authorized_keys || echo '${publicKey}' >> ~/.ssh/authorized_keys)`,
    };
  }

  async savedHosts(): Promise<SavedHost[]> {
    return structuredClone(this.hosts);
  }

  async saveHost(host: SavedHost): Promise<SavedHost[]> {
    const h = { ...structuredClone(host), host: host.host.trim(), user: host.user.trim() };
    if (!h.host || /\s/.test(h.host)) throw new Error('the host name is empty or has spaces');
    if (!h.user || /\s/.test(h.user)) throw new Error('the host user is empty or has spaces');
    if (!h.label.trim()) h.label = h.host;
    if (h.jump && !h.jump.host.trim()) h.jump = null;
    if (h.jump && !h.jump.user) h.jump.user = h.user;
    if (!h.id) {
      let n = 1;
      while (this.hosts.some((x) => x.id === `h${n}`)) n++;
      h.id = `h${n}`;
    }
    const i = this.hosts.findIndex((x) => x.id === h.id);
    if (i >= 0) this.hosts[i] = h;
    else this.hosts.push(h);
    return structuredClone(this.hosts);
  }

  async deleteHost(id: string): Promise<SavedHost[]> {
    this.hosts = this.hosts.filter((h) => h.id !== id);
    this.trusted.delete(id);
    return structuredClone(this.hosts);
  }

  async forgetHostKey(id: string): Promise<void> {
    this.trusted.delete(id);
  }

  async copyText(text: string): Promise<void> {
    await globalThis.navigator?.clipboard?.writeText(text);
  }

  async pasteText(): Promise<string> {
    return (await globalThis.navigator?.clipboard?.readText()) ?? '';
  }

  async answerPrompt(id: number, answer: string | null): Promise<void> {
    const resolve = this.prompts.get(id);
    if (!resolve) throw new Error(`no pending prompt ${id}`);
    this.prompts.delete(id);
    resolve(answer);
  }

  async localNetworkStatus(host: string | null = null): Promise<LocalNetworkReport> {
    return { status: 'notApplicable', host, addresses: [], private: false };
  }

  async openLocalNetworkSettings(): Promise<void> {}

  private portsState(added?: Forward[]): PortsState {
    const remote = this.target.kind !== 'local';
    return structuredClone({ remote, detected: this.detected, forwards: remote ? this.forwards : [], ...(added?.length ? { added } : {}) });
  }

  private publishPorts(added?: Forward[]) {
    const s = this.portsState(added);
    for (const w of this.portWatchers) w(s);
  }

  private autoForward(): Forward[] {
    if (this.target.kind === 'local' || !this.portConfig.autoForward) return [];
    const added: Forward[] = [];
    for (const p of this.detected) {
      if (this.portConfig.ignore.includes(p.port) || this.unforwarded.has(p.port) || this.forwards.some((f) => f.remotePort === p.port)) continue;
      const f = this.makeForward(p.port, null, 'auto');
      this.forwards.push(f);
      added.push(f);
    }
    return added;
  }

  private makeForward(remotePort: number, localPort: number | null, source: Forward['source']): Forward {
    const p = this.detected.find((x) => x.port === remotePort);
    let local = localPort ?? remotePort;
    while (localPort === null && this.forwards.some((f) => f.localPort === local)) local++;
    const localAddr = this.portConfig.localhostOnly ? '127.0.0.1' : '0.0.0.0';
    return { remotePort, localPort: local, localAddr, url: `http://localhost:${local}`, source, process: p?.process, pid: p?.pid, termId: p?.termId, project: p?.project };
  }

  watchPorts(onState: (s: PortsState) => void): () => void {
    this.portWatchers.add(onState);
    onState(this.portsState());
    const later = (ms: number, fn: () => void) => this.portTimers.push(setTimeout(fn, this.delayMs ? ms : 0));
    later(800, () => this.publishPorts(this.autoForward()));
    if (this.live) {
      later(9000, () => {
        if (this.detected.some((p) => p.port === 3000)) return;
        const term = this.terms.find((x) => !x.exited) ?? this.terms[0];
        this.detected = [...this.detected, mockPort(3000, 'next-server', term, '0.0.0.0')].sort((a, b) => a.port - b.port);
        this.publishPorts(this.autoForward());
      });
    }
    return () => {
      this.portWatchers.delete(onState);
      if (this.portWatchers.size === 0) {
        for (const x of this.portTimers) clearTimeout(x);
        this.portTimers = [];
      }
    };
  }

  async configurePorts(s: PortSettings): Promise<void> {
    this.portConfig = structuredClone(s);
    this.publishPorts(this.autoForward());
  }

  async forwardPort(remotePort: number, localPort: number | null = null): Promise<Forward> {
    if (this.target.kind === 'local') throw new Error('port forwarding needs a remote host');
    if (this.forwards.some((f) => f.remotePort === remotePort)) throw new Error(`port ${remotePort} is already forwarded`);
    if (localPort !== null && this.forwards.some((f) => f.localPort === localPort)) throw new Error(`bind 127.0.0.1:${localPort}: address already in use`);
    const f = this.makeForward(remotePort, localPort, 'manual');
    this.forwards.push(f);
    this.unforwarded.delete(remotePort);
    this.publishPorts();
    return structuredClone(f);
  }

  private killPortProcess(raw: string, body: any): Port {
    const port = /^\d+$/.test(raw) ? Number(raw) : NaN;
    if (!(port >= 0 && port <= 65535)) throw new DaemonError(400, `bad port ${JSON.stringify(raw)}`);
    const pid = body?.pid;
    if (!Number.isInteger(pid) || pid < 0 || pid > 0xffffffff) throw new DaemonError(400, 'pid (number) required');
    if (body?.confirm !== true) throw new DaemonError(400, 'kill needs {"confirm":true}');
    const p = this.detected.find((x) => x.port === port && x.pid === pid);
    if (!p) throw new DaemonError(404, `no process ${pid} listening on port ${port} in an Overseer terminal`);
    this.detected = this.detected.filter((x) => x !== p);
    this.publishPorts();
    return structuredClone(p);
  }

  async unforwardPort(remotePort: number): Promise<void> {
    this.forwards = this.forwards.filter((f) => f.remotePort !== remotePort);
    this.unforwarded.add(remotePort);
    this.publishPorts();
  }

  async disconnect(): Promise<void> {
    this.stop();
    for (const x of this.portTimers) clearTimeout(x);
    this.portTimers = [];
  }

  async sshHosts(): Promise<SshHost[]> {
    return [
      { alias: 'devbox', hostName: 'devbox.internal', user: 'alice', port: 22 },
      { alias: 'runner-1', hostName: '10.0.4.21', user: 'ci' },
      { alias: 'gpu', hostName: 'gpu.lab.example.com', user: 'alice', port: 2222 },
    ];
  }
}

function mockPort(port: number, process: string, term: Term | undefined, addr = '127.0.0.1'): Port {
  return {
    port,
    addr,
    pid: (term?.pid ?? 4000) + port % 97,
    process,
    termId: term?.id ?? 'term-0',
    lineId: term?.lineId ?? undefined,
    project: term?.cwd,
    since: new Date().toISOString(),
  };
}

function seedPorts(terms: Term[]): Port[] {
  const live = terms.filter((x) => !x.exited);
  return [mockPort(5173, 'node', live[0] ?? terms[0]), mockPort(8000, 'python3', live[1] ?? terms[0], '0.0.0.0')];
}

function fakeRun(cmd: string, agent: string | null, cwd: string): string {
  if (!cmd) return '';
  if (agent) return `\x1b[2m● Thinking…\x1b[0m\r\nSure. I will ${cmd.replace(/^\w/, (c) => c.toLowerCase())} and report back.\r\n`;
  const [bin] = cmd.split(/\s+/);
  switch (bin) {
    case 'ls':
      return 'Cargo.toml  README.md  app  docs  src  tests\r\n';
    case 'pwd':
      return `${cwd}\r\n`;
    case 'echo':
      return cmd.slice(5) + '\r\n';
    case 'git':
      return 'On branch feat/app\r\nnothing to commit, working tree clean\r\n';
    case 'help':
      return 'Demo shell: ls, pwd, echo, git, exit\r\n';
    default:
      return `zsh: command not found: ${bin}\r\n`;
  }
}

function mockHooks(): HooksStatus {
  const row = (agent: string, name: string, mechanism: 'hooks' | 'plugin' | 'polling', configured: boolean, path?: string, note?: string) => ({
    agent,
    name,
    mechanism,
    cliInstalled: configured,
    configured,
    installed: false,
    current: false,
    ...(path ? { path } : {}),
    ...(note ? { note } : {}),
  });
  return {
    shim: '/home/dev/.overseer/bin/overseer',
    agents: [
      row('claude', 'Claude Code', 'hooks', true, '/home/dev/.claude/settings.json'),
      row('codex', 'Codex', 'hooks', true, '/home/dev/.codex/hooks.json', 'Codex runs edited hooks only after you trust them again'),
      row('gemini', 'Gemini CLI', 'hooks', false, '/home/dev/.gemini/settings.json'),
      row('opencode', 'opencode', 'plugin', true, '/home/dev/.config/opencode/plugins/overseer.js'),
      row('agy', 'Antigravity', 'polling', false, undefined, 'No stable hook file for the Antigravity CLI yet'),
    ],
  };
}

const MOCK_DIRS: Record<string, string[]> = {
  '/': ['etc', 'home', 'opt', 'srv', 'tmp', 'usr', 'var'],
  '/home': ['alice'],
  [HOME]: ['.cache', '.config', '.ssh', 'Desktop', 'Documents', 'Downloads', 'Music', 'Pictures', 'infra-lab', 'scratch', 'src'],
  [`${HOME}/Documents`]: ['Github', 'invoices', 'notes'],
  [`${HOME}/Documents/Github`]: ['uni', 'perso', 'work'],
  [`${HOME}/Documents/Github/work`]: ['webshop', 'webshop-docs', 'data-pipelines', 'infra', 'overseer'],
  [`${HOME}/Documents/Github/uni`]: ['compilers', 'os-labs', 'reports'],
  [`${HOME}/Documents/Github/perso`]: ['blog', 'dotfiles', 'homelab', 'raytracer'],
  [`${HOME}/src`]: ['advent-of-code', 'go-playground', 'my-project', 'rust-scratch', 'zig-tests'],
  [`${HOME}/.config`]: ['nvim', 'opencode', 'zed'],
};
const MOCK_GIT = new Set(
  [...Object.values(REPOS).map((r) => r.path), `${HOME}/infra-lab`, `${HOME}/src/advent-of-code`, `${HOME}/src/my-project`, `${HOME}/src/rust-scratch`, `${HOME}/.config/nvim`]
    .concat(['webshop-docs', 'data-pipelines', 'infra'].map((n) => `${HOME}/Documents/Github/work/${n}`))
    .concat(['blog', 'dotfiles', 'homelab', 'raytracer'].map((n) => `${HOME}/Documents/Github/perso/${n}`)),
);

const query = typeof location !== 'undefined' ? new URLSearchParams(location.search) : new URLSearchParams();
const latency = Number(query.get('latency')) || 0;

export interface Faults {
  rate: number;
  on: string;
  hang: string;
  hangMs: number;
}

export const faults: Faults = { rate: Number(query.get('fail')) || 0, on: query.get('failOn') ?? '', hang: query.get('hang') ?? '', hangMs: 30_000 };

export async function inject(what: string, f: Faults = faults, roll: () => number = Math.random) {
  if (f.hang && what.includes(f.hang)) {
    await new Promise((r) => setTimeout(r, f.hangMs));
    throw new DaemonError(0, `${what}: timed out`);
  }
  if (f.rate > 0 && (!f.on || what.includes(f.on)) && roll() < f.rate) throw new DaemonError(503, `${what}: HTTP 503 (injected)`);
}

async function lag(what: string) {
  if (latency) {
    (globalThis as { __sbLog?: string[] }).__sbLog?.push(what);
    await new Promise((r) => setTimeout(r, latency));
  }
  if (faults.rate || faults.hang) await inject(what);
}

function mockDirs(q: URLSearchParams): DirListing {
  const raw = (q.get('path') ?? '').trim();
  if (raw && !raw.startsWith('/') && !raw.startsWith('~')) throw new DaemonError(400, `${raw}: use an absolute path or ~`);
  const path = raw.replace(/^~(?=\/|$)/, HOME).replace(/(.)\/+$/, '$1') || HOME;
  const sub = MOCK_DIRS[path];
  const parent = path === '/' ? null : path.slice(0, path.lastIndexOf('/')) || '/';
  if (!sub && !MOCK_DIRS[parent ?? '']?.includes(path.slice(path.lastIndexOf('/') + 1))) throw new DaemonError(404, `${path}: No such file or directory`);
  const hidden = q.get('hidden') === '1';
  const join = (n: string) => (path === '/' ? `/${n}` : `${path}/${n}`);
  const dirs = (sub ?? [])
    .filter((n) => hidden || !n.startsWith('.'))
    .sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()))
    .map((name) => ({ name, path: join(name), git: MOCK_GIT.has(join(name)) }));
  return { path, parent, home: HOME, dirs, truncated: false };
}
