import { CMD } from './tauri-commands';
import { isTauri, DaemonError, type DaemonClient } from './client';
import { type Sub, t } from './i18n.svelte';
import { errorText, forwardWarning, Limiter, type DiagInfo, type LogTail, type PreviousSession, type ServerLog } from './diag';

export type DiagMode = 'previous' | 'live';

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

const MOCK_LINES = [
  '2026-01-01T09:00:00.000Z INFO  app: Overseer 0.0.0 starting (browser preview)',
  '2026-01-01T09:00:01.000Z INFO  conn: local connected, server 0.0.0',
  '2026-01-01T09:00:05.000Z WARN  perf: daemon GET /v1/terms took 2.4 s',
  '2026-01-01T09:00:09.000Z ERROR webview: TypeError: x is undefined',
  '    at f (App.svelte:1:1)',
];

export const diagBackend = {
  tail: (lines: number): Promise<LogTail> => (isTauri() ? call<LogTail>(CMD.diagTail, { lines }) : Promise.resolve({ path: 'app.log', lines: MOCK_LINES })),
  info: (): Promise<DiagInfo | null> => (isTauri() ? call<DiagInfo>(CMD.diagInfo) : Promise.resolve(null)),
  previous: (): Promise<PreviousSession | null> => (isTauri() ? call<PreviousSession | null>(CMD.diagPrevious) : Promise.resolve(null)),
  dismiss: (): Promise<void> => (isTauri() ? call<void>(CMD.diagDismiss) : Promise.resolve()),
  save: (text: string): Promise<string> => call<string>(CMD.diagSave, { text }),
  openDir: (): Promise<void> => call<void>(CMD.diagOpenDir),
  sessionEnd: (reason: string): Promise<void> => (isTauri() ? call<void>(CMD.diagSessionEnd, { reason }) : Promise.resolve()),
};

export async function fetchServerLog(client: DaemonClient | null, connected: boolean, tail = 300): Promise<ServerLog> {
  if (!client || !connected) return { status: 'offline' };
  try {
    return { status: 'ok', tail: await client.request<LogTail>('GET', `/v1/logs?tail=${tail}`) };
  } catch (e) {
    if (e instanceof DaemonError && (e.status === 404 || e.status === 405)) return { status: 'unsupported' };
    return { status: 'error', message: errorText(e).message };
  }
}

class Diag {
  mode = $state<DiagMode | null>(null);
  previous = $state.raw<PreviousSession | null>(null);

  open(mode: DiagMode) {
    this.mode = mode;
  }

  close() {
    if (this.mode === 'previous') {
      this.previous = null;
      void diagBackend.dismiss().catch(() => {});
    }
    this.mode = null;
  }
}

export const diag = new Diag();

const limiter = new Limiter();
let forwarding = false;

function forward(level: 'error' | 'warn' | 'info', message: string, stack?: string) {
  if (forwarding || !limiter.allow(`${level}:${message}`, Date.now())) return;
  forwarding = true;
  const dropped = limiter.takeDropped();
  if (dropped) void call(CMD.diagLog, { level: 'warn', message: `${dropped} frontend messages not forwarded (rate limit)`, stack: null }).catch(() => {});
  void call(CMD.diagLog, { level, message, stack: stack ?? null }).catch(() => {});
  forwarding = false;
}

function argText(a: unknown): string {
  if (a instanceof Error) return `${a.name}: ${a.message}`;
  if (typeof a === 'string') return a;
  return errorText(a).message;
}

export function startDiagnostics(viewOf: () => string): () => void {
  if (!isTauri() || typeof window === 'undefined') return () => {};
  const stops: (() => void)[] = [];
  const onError = (e: ErrorEvent) => {
    const x = e.error ? errorText(e.error) : { message: e.message, stack: undefined };
    const where = e.filename ? ` (${e.filename}:${e.lineno}:${e.colno})` : '';
    forward('error', `Uncaught ${x.message}${where}`, x.stack);
  };
  const onRejection = (e: PromiseRejectionEvent) => {
    const x = errorText(e.reason);
    forward('error', `Unhandled rejection: ${x.message}`, x.stack);
  };
  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onRejection);
  stops.push(() => {
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onRejection);
  });

  const { error, warn } = console;
  console.error = (...args: unknown[]) => {
    error.apply(console, args);
    const err = args.find((a): a is Error => a instanceof Error);
    forward('error', args.map(argText).join(' '), err?.stack);
  };
  console.warn = (...args: unknown[]) => {
    warn.apply(console, args);
    const text = args.map(argText).join(' ');
    if (forwardWarning(text)) forward('warn', text);
  };
  stops.push(() => {
    console.error = error;
    console.warn = warn;
  });

  const onVisibility = () => void call(CMD.diagVisibility, { visible: document.visibilityState === 'visible' }).catch(() => {});
  document.addEventListener('visibilitychange', onVisibility);
  stops.push(() => document.removeEventListener('visibilitychange', onVisibility));

  let unlisten: (() => void) | null = null;
  let stopped = false;
  void import('@tauri-apps/api/event').then(async ({ listen }) => {
    const u = await listen('diag:ping', () => void call(CMD.diagPong, { view: viewOf() }).catch(() => {}));
    if (stopped) u();
    else unlisten = u;
  });
  stops.push(() => {
    stopped = true;
    unlisten?.();
  });
  void call(CMD.diagPong, { view: viewOf() }).catch(() => {});

  void diagBackend
    .previous()
    .then((p) => {
      if (!p) return;
      diag.previous = p;
      diag.open('previous');
    })
    .catch(() => {});
  return () => stops.forEach((s) => s());
}

type DKey = Sub<'diag'>;

export function dt(key: DKey, params?: Record<string, string | number>): string {
  return t(`diag.${key}`, params);
}
