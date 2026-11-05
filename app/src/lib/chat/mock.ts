import type { ChatEntry, ChatPage, CustomCommand, ScreenText } from '../types';

export const MOCK_SCREEN = [
  '● Bash(cargo test --offline term::)',
  '',
  '╭──────────────────────────────────────────────────────────────╮',
  '│ Bash command                                                 │',
  '│                                                              │',
  '│   cargo test --offline term::                                │',
  '│   Run the terminal tests                                     │',
  '│                                                              │',
  '│ Do you want to proceed?                                      │',
  '│ ❯ 1. Yes                                                     │',
  "│   2. Yes, and don't ask again for cargo test commands       │",
  '│   3. No, and tell Claude what to do differently (esc)        │',
  '╰──────────────────────────────────────────────────────────────╯',
].join('\n');

export const MOCK_QUESTION = [
  '●  Two more choices before I write the plan.',
  '',
  '←  ☒ Epic  □ Backing svc  □ Seed  □ Auth  ✔ Submit  →',
  '',
  'Where do the backing services of a preview env run?',
  '',
  ' 1. Sidecars in the pod (Recommended)',
  '    Each env self-contained, data on the env PVC, seeded from a dump at first start. Simple',
  '    teardown = delete namespace',
  ' 2. Shared instances',
  '    One shared Postgres/MinIO on the cluster, per-env DB copy (pg_dump | psql like feedback-hub',
  '    today) and per-env bucket',
  ' 3. Shared DB, sidecar rest',
  '    Shared Postgres with per-env DB copy, MinIO and mailpit as sidecars',
  '❯ 4. Type something.',
  '────────────────────────────────────────',
  ' 5. Chat about this',
  '',
  'Enter to select · Tab/Arrow keys to navigate · ctrl+g to edit in Vim · Esc to cancel',
].join('\n');

export const MOCK_COMMANDS: CustomCommand[] = [
  { name: '/review-pr', description: 'Review the open pull request', source: 'user' },
  { name: '/release-notes', description: 'Draft release notes from the changelog', source: 'project' },
];

function at(now: number, minutesAgo: number): string {
  return new Date(now - minutesAgo * 60_000).toISOString();
}

export function seedChat(now = Date.now()): ChatEntry[] {
  const e: Omit<ChatEntry, 'seq' | 'rev'>[] = [
    { id: 'u1', kind: 'user', at: at(now, 14), text: 'The PTY resize is too eager over SSH. Debounce it to 50 ms and add a test.', images: 2, imagesAt: 0 },
    { id: 'a1', kind: 'assistant', at: at(now, 13), model: 'claude-opus-4', text: "I'll look at how resizes reach the PTY first." },
    { id: 't1', kind: 'tool', at: at(now, 13), tool: { name: 'Read', kind: 'read', label: 'Read term.rs', path: '/w/src/term.rs', state: 'done', output: 'pub fn resize(&self, cols: u16, rows: u16)…' } },
    { id: 't2', kind: 'tool', at: at(now, 12), tool: { name: 'Grep', kind: 'search', label: 'fn resize', state: 'done', output: 'src/term.rs\nsrc/daemon.rs' } },
    {
      id: 'td1',
      kind: 'todos',
      at: at(now, 12),
      todos: [
        { text: 'Debounce resize', state: 'completed' },
        { text: 'Add a test', state: 'in_progress' },
        { text: 'Update the changelog', state: 'pending' },
      ],
    },
    {
      id: 't3',
      kind: 'tool',
      at: at(now, 11),
      tool: {
        name: 'Edit',
        kind: 'edit',
        label: 'Edit term.rs',
        path: '/w/src/term.rs',
        state: 'done',
        diff: { old: '    pub fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {\n        self.pty.resize(cols, rows)\n    }', new: '    pub fn resize(&mut self, cols: u16, rows: u16) {\n        self.pending = Some((cols, rows));\n    }' },
      },
    },
    {
      id: 't4',
      kind: 'tool',
      at: at(now, 9),
      tool: {
        name: 'Task',
        kind: 'task',
        label: 'Find other resize callers',
        detail: 'Explore',
        state: 'done',
        output: 'Only daemon.rs and the ws handler call resize.',
        agent: { agentId: 'a4f2e91', type: 'Explore', description: 'Find other resize callers', state: 'done', background: false, startedAt: at(now, 9), endedAt: at(now, 8), tokens: 18_400, toolCount: 6, lastActivity: null },
      },
    },
    { id: 't5', kind: 'tool', at: at(now, 6), tool: { name: 'Bash', kind: 'bash', label: 'Run the tests', command: 'cargo test --offline term::', state: 'failed', output: 'running 3 tests\ntest resize_is_debounced ... FAILED\n\nfailures:\n    resize_is_debounced', truncated: false } },
    {
      id: 'a2',
      kind: 'assistant',
      at: at(now, 5),
      model: 'claude-opus-4',
      text: 'The test fails because `flush_resize` runs before the quiet period. Fix:\n\n```rust\nif self.last_resize.elapsed() < Duration::from_millis(50) {\n    return Ok(false);\n}\n```\n\n- keeps the **last** size\n- drops intermediate ones\n\nSee [the ADR](https://example.com/adr).',
    },
    {
      id: 't7',
      kind: 'tool',
      at: at(now, 4),
      tool: {
        name: 'Agent',
        kind: 'task',
        label: 'Run markdown and termlinks tests',
        detail: 'general-purpose',
        state: 'running',
        agent: { agentId: 'a9c03b7', type: 'general-purpose', description: 'Run markdown and termlinks tests', state: 'running', background: true, startedAt: at(now, 4.03), endedAt: null, tokens: 88_300, toolCount: 14, lastActivity: 'pnpm exec vitest run src/lib/chat' },
      },
    },
    { id: 't6', kind: 'tool', at: at(now, 1), tool: { name: 'Bash', kind: 'bash', label: 'Run the tests', command: 'cargo test --offline term::', state: 'running' } },
  ];
  return e.map((x, i) => ({ ...x, seq: i + 1, rev: i + 1 }) as ChatEntry);
}

export function seedAgentChat(agentId: string, now = Date.now()): ChatEntry[] | null {
  const running = agentId === 'a9c03b7';
  if (!running && agentId !== 'a4f2e91') return null;
  const e: Omit<ChatEntry, 'seq' | 'rev'>[] = running
    ? [
        { id: 's1', kind: 'user', at: at(now, 4), text: 'Run the markdown and termlinks tests and report failures.' },
        { id: 's2', kind: 'assistant', at: at(now, 3.8), model: 'claude-opus-4', text: 'Running the two suites.' },
        { id: 's3', kind: 'tool', at: at(now, 3.5), tool: { name: 'Bash', kind: 'bash', label: 'Run markdown tests', command: 'pnpm exec vitest run src/lib/chat/markdown.test.ts', state: 'done', output: '✓ 9 tests passed' } },
        { id: 's4', kind: 'tool', at: at(now, 0.2), tool: { name: 'Bash', kind: 'bash', label: 'pnpm exec vitest run src/lib/chat', command: 'pnpm exec vitest run src/lib/chat', state: 'running' } },
      ]
    : [
        { id: 's1', kind: 'user', at: at(now, 9), text: 'Find every caller of resize.' },
        { id: 's2', kind: 'tool', at: at(now, 9), tool: { name: 'Grep', kind: 'search', label: 'resize(', state: 'done', output: 'src/daemon.rs\nsrc/ws.rs' } },
        { id: 's3', kind: 'assistant', at: at(now, 8), model: 'claude-opus-4', text: 'Only `daemon.rs` and the ws handler call **resize**.' },
      ];
  return e.map((x, i) => ({ ...x, seq: i + 1, rev: i + 1 }) as ChatEntry);
}

export function chatPage(lineId: string, agent: string, all: ChatEntry[], q: URLSearchParams): ChatPage {
  const limit = Math.min(Number(q.get('limit') ?? 200) || 200, 500);
  const cursor = all.reduce((m, e) => Math.max(m, e.rev), 0);
  const model = [...all].reverse().find((e) => e.model)?.model ?? null;
  const base = { lineId, agent, cursor, reset: false, model };
  if (q.has('after')) {
    const after = Number(q.get('after'));
    return { ...base, more: false, entries: all.filter((e) => e.rev > after) };
  }
  const before = q.has('before') ? Number(q.get('before')) : Infinity;
  const older = all.filter((e) => e.seq < before);
  const entries = older.slice(-limit);
  return { ...base, more: older.length > entries.length, entries };
}

export function mockScreen(lineId: string): ScreenText {
  return lineId === 'ln_01' ? { lineId, text: MOCK_QUESTION, cols: 100, rows: 19, source: 'term' } : { lineId, text: MOCK_SCREEN, cols: 64, rows: 13, source: 'term' };
}
