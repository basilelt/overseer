export type ExitKind = 'crash' | 'freeze' | 'killed';
export type LevelFilter = 'all' | 'info' | 'warn' | 'error';
export type Level = 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';

export interface PreviousSession {
  kind: ExitKind;
  startedAt: number;
  endedAt: string | null;
  detectedAt: number;
  version: string;
  frozenSecs: number;
  view: string;
  crashReport: string | null;
  appLog: string[];
}

export interface DiagInfo {
  version: string;
  serverVersion: string;
  os: string;
  arch: string;
  logDir: string;
}

export interface LogTail {
  path: string;
  lines: string[];
}

export interface LogLine {
  text: string;
  level: Level;
}

export type ServerLog = { status: 'ok'; tail: LogTail } | { status: 'unsupported' } | { status: 'offline' } | { status: 'error'; message: string };

const HEAD = /^\d{4}-\d{2}-\d{2}T\S+\s+(DEBUG|INFO|WARN|ERROR|TRACE)\b/;

export function parseLines(lines: string[]): LogLine[] {
  let level: Level = 'INFO';
  return lines.map((text) => {
    const m = HEAD.exec(text);
    if (m) level = m[1] === 'TRACE' ? 'DEBUG' : (m[1] as Level);
    else if (!/^\s/.test(text) && /panicked at|^thread '/.test(text)) level = 'ERROR';
    return { text, level };
  });
}

const RANK: Record<Level, number> = { DEBUG: 0, INFO: 1, WARN: 2, ERROR: 3 };
const MIN: Record<LevelFilter, number> = { all: 0, info: 1, warn: 2, error: 3 };

export function filterLines(lines: LogLine[], level: LevelFilter, query: string): LogLine[] {
  const q = query.trim().toLowerCase();
  return lines.filter((l) => RANK[l.level] >= MIN[level] && (!q || l.text.toLowerCase().includes(q)));
}

export function iso(ms: number): string {
  return Number.isFinite(ms) && ms > 0 ? new Date(ms).toISOString() : '?';
}

export interface BundleInput {
  generatedAt: number;
  info: DiagInfo | null;
  previous: PreviousSession | null;
  appLog: string[];
  server: ServerLog | null;
  host?: string;
}

const KIND_TEXT: Record<ExitKind, string> = {
  crash: 'crashed (Rust panic)',
  freeze: 'was frozen (UI unresponsive) when it ended',
  killed: 'was killed or force-quit (no clean exit)',
};

export function describeExit(p: PreviousSession): string {
  const frozen = p.kind === 'freeze' && p.frozenSecs ? `, unresponsive for ${p.frozenSecs} s` : '';
  const view = p.view ? `, view: ${p.view}` : '';
  return `${KIND_TEXT[p.kind]}${frozen}${view}`;
}

function section(title: string, body: string[]): string {
  return [`===== ${title} =====`, ...(body.length ? body : ['(empty)']), ''].join('\n');
}

export function formatBundle(b: BundleInput): string {
  const head = ['Overseer diagnostic bundle', `Generated: ${iso(b.generatedAt)}`];
  if (b.info) {
    head.push(`App: ${b.info.version} (server ${b.info.serverVersion}), ${b.info.os} ${b.info.arch}`);
    if (b.info.logDir) head.push(`Log folder: ${b.info.logDir}`);
  }
  if (b.host) head.push(`Host: ${b.host}`);
  const parts = [head.join('\n') + '\n'];
  if (b.previous) {
    const p = b.previous;
    parts.push(
      section('Previous session', [
        `Started: ${iso(p.startedAt)}`,
        `Last log line: ${p.endedAt ?? '?'}`,
        `Version: ${p.version || '?'}`,
        `Outcome: ${describeExit(p)}`,
      ]),
    );
    if (p.crashReport) parts.push(section('Crash report', p.crashReport.trimEnd().split('\n')));
    parts.push(section(`App log before the exit (last ${p.appLog.length} lines)`, p.appLog));
  } else {
    parts.push(section(`App log (last ${b.appLog.length} lines)`, b.appLog));
  }
  parts.push(section('Server log', serverBody(b.server)));
  return parts.join('\n').trimEnd() + '\n';
}

function serverBody(s: ServerLog | null): string[] {
  if (!s || s.status === 'offline') return ['(not connected to a server)'];
  if (s.status === 'unsupported') return ['(the server is too old to send its log; it is in ~/.overseer/daemon.log on the host)'];
  if (s.status === 'error') return [`(could not fetch the server log: ${s.message})`];
  return [`# ${s.tail.path} (last ${s.tail.lines.length} lines)`, ...s.tail.lines];
}

export function errorText(e: unknown): { message: string; stack?: string } {
  if (e instanceof Error) return { message: `${e.name}: ${e.message}`, stack: e.stack };
  if (typeof e === 'string') return { message: e };
  try {
    return { message: JSON.stringify(e) ?? String(e) };
  } catch {
    return { message: String(e) };
  }
}

export class Limiter {
  private start = 0;
  private used = 0;
  private seen = new Map<string, number>();
  dropped = 0;

  constructor(
    private burst = 20,
    private windowMs = 10_000,
  ) {}

  allow(key: string, now: number): boolean {
    if (now - this.start >= this.windowMs) {
      this.start = now;
      this.used = 0;
      this.seen.clear();
    }
    const n = (this.seen.get(key) ?? 0) + 1;
    this.seen.set(key, n);
    if (this.used >= this.burst || n > 3) {
      this.dropped++;
      return false;
    }
    this.used++;
    return true;
  }

  takeDropped(): number {
    const d = this.dropped;
    this.dropped = 0;
    return d;
  }
}

const FORWARDED_WARNINGS = /svelte|ghostty|wasm|webgl|tauri|ipc|terminal/i;

export function forwardWarning(text: string): boolean {
  return FORWARDED_WARNINGS.test(text);
}
