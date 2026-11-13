import { describe, expect, it } from 'vitest';
import type { PreviousSession } from './diag';
import { formatBundle } from './diag-bundle';

const LINES = [
  '2026-10-29T10:00:00.000Z INFO  app: Overseer 0.4.0 starting',
  '2026-10-29T10:00:01.000Z WARN  ui: UI unresponsive for 6 s (view: agents)',
  '2026-10-29T10:00:02.000Z ERROR webview: TypeError: x is undefined',
  '    at render (App.svelte:10:2)',
  '2026-10-29T10:00:03.000Z DEBUG conn: noise',
  '2026-10-29T10:00:04.000Z INFO listening on /tmp/s.sock',
  "thread 'main' panicked at src/x.rs:1:1:",
];

const previous: PreviousSession = {
  kind: 'freeze',
  startedAt: Date.UTC(2026, 9, 29, 9, 0, 0),
  endedAt: '2026-10-29T10:00:04.000Z',
  detectedAt: Date.UTC(2026, 9, 29, 11, 0, 0),
  version: '0.4.0',
  frozenSecs: 14,
  view: 'agents/term/connected',
  crashReport: null,
  appLog: LINES.slice(0, 2),
};

describe('diagnostic bundle', () => {
  it('formats a single plain-text block with every section', () => {
    const text = formatBundle({
      generatedAt: Date.UTC(2026, 9, 29, 11, 0, 0),
      info: { version: '0.4.0', serverVersion: '0.4.0', os: 'macos', arch: 'aarch64', logDir: '/Users/u/Library/Logs/io.github.basilelt.overseer' },
      previous: { ...previous, kind: 'crash', crashReport: 'Overseer crash report\nMessage: boom\n' },
      appLog: ['ignored when a previous session is shown'],
      server: { status: 'ok', tail: { path: '/home/u/.overseer/daemon.log', lines: ['2026-10-29T10:00:00.000Z INFO listening'] } },
      host: 'u@w:22',
    });
    expect(text.startsWith('Overseer diagnostic bundle\nGenerated: 2026-10-29T11:00:00.000Z\nApp: 0.4.0 (server 0.4.0), macos aarch64\n')).toBe(true);
    expect(text).toContain('Host: u@w:22');
    expect(text).toContain('===== Previous session =====\nStarted: 2026-10-29T09:00:00.000Z\nLast log line: 2026-10-29T10:00:04.000Z\nVersion: 0.4.0\nOutcome: crashed (Rust panic)');
    expect(text).toContain('===== Crash report =====\nOverseer crash report\nMessage: boom\n');
    expect(text).toContain('===== App log before the exit (last 2 lines) =====');
    expect(text).not.toContain('ignored when a previous session');
    expect(text).toContain('===== Server log =====\n# /home/u/.overseer/daemon.log (last 1 lines)\n2026-10-29T10:00:00.000Z INFO listening');
    expect(text.endsWith('\n')).toBe(true);
  });

  it('explains a missing server log', () => {
    const base = { generatedAt: 0, info: null, previous: null, appLog: [] };
    expect(formatBundle({ ...base, server: null })).toContain('(not connected to a server)');
    expect(formatBundle({ ...base, server: { status: 'unsupported' } })).toContain('too old to send its log');
    expect(formatBundle({ ...base, server: { status: 'error', message: 'HTTP 500' } })).toContain('could not fetch the server log: HTTP 500');
    expect(formatBundle({ ...base, server: { status: 'offline' } })).toContain('===== App log (last 0 lines) =====\n(empty)');
  });
});
