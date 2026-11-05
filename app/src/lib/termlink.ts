import type { TermIO } from './client';
import { attemptAfterDetach } from './conn';

export interface LinkDeps {
  open(onData: (bytes: Uint8Array) => void, onClose: () => void): Promise<TermIO>;
  reset(): void;
  data(bytes: Uint8Array): void;
  up(io: TermIO): void;
  down(): void;
  failed(error: unknown): void;
  retryMs(attempt: number, error?: unknown): number | null;
  ended(): void;
  now?: () => number;
}

export interface TermLink {
  attach(): Promise<void>;
  reattach(): void;
  resume(): void;
  dispose(): void;
}

// One live connection per terminal: every attach supersedes the previous one, which is closed and
// whose late data, close or error is ignored, and at most one retry is pending at a time.
export function termLink(deps: LinkDeps): TermLink {
  const now = deps.now ?? Date.now;
  let gen = 0;
  let io: TermIO | null = null;
  let pending = false;
  let since = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  let stalled = false;

  const drop = () => {
    const prev = io;
    io = null;
    if (prev) {
      deps.down();
      prev.close();
    }
  };

  const retry = (attempt: number, error?: unknown) => {
    if (disposed) return;
    if (error !== undefined) deps.failed(error);
    const wait = deps.retryMs(attempt, error);
    if (wait === null) {
      stalled = error !== undefined;
      if (error === undefined) deps.ended();
      return;
    }
    timer = setTimeout(() => void attach(attempt + 1), wait);
  };

  const detached = (attempt: number) => {
    io = null;
    deps.down();
    retry(attemptAfterDetach(attempt, now() - since));
  };

  const attach = async (attempt: number): Promise<void> => {
    if (disposed) return;
    clearTimeout(timer);
    timer = undefined;
    stalled = false;
    const g = ++gen;
    drop();
    pending = true;
    deps.reset();
    let opened = false;
    let closedEarly = false;
    const onClose = () => {
      if (g !== gen) return;
      if (!opened) closedEarly = true;
      else detached(attempt);
    };
    let conn: TermIO;
    try {
      conn = await deps.open((bytes) => {
        if (g === gen) deps.data(bytes);
      }, onClose);
    } catch (e) {
      if (g !== gen || disposed) return;
      pending = false;
      return retry(attempt, e);
    }
    if (disposed || g !== gen) return conn.close();
    pending = false;
    opened = true;
    since = now();
    if (closedEarly) {
      conn.close();
      return retry(attemptAfterDetach(attempt, 0));
    }
    io = conn;
    deps.up(conn);
  };

  return {
    attach: () => attach(0),
    reattach() {
      if (disposed || (!io && !pending)) return;
      void attach(0);
    },
    resume() {
      if (disposed || !stalled) return;
      void attach(0);
    },
    dispose() {
      disposed = true;
      gen++;
      clearTimeout(timer);
      const prev = io;
      io = null;
      prev?.close();
    },
  };
}
