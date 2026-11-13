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
