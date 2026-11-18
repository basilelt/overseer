const REPORT = /^\x1b\[(?:\d+;\d+R|0n|\?[\d;]*c)$/;

export interface ReplayGuard {
  begin(): void;
  chunk(): void;
  allow(input: string): boolean;
  dispose(): void;
}

export function replayGuard(settleMs = 150, now: () => number = Date.now): ReplayGuard {
  let replaying = false;
  let last = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const arm = (wait: number) => {
    timer = setTimeout(() => {
      const idle = now() - last;
      if (idle >= settleMs) {
        replaying = false;
        timer = undefined;
      } else arm(settleMs - idle);
    }, wait);
  };

  return {
    begin() {
      clearTimeout(timer);
      replaying = true;
      last = now();
      arm(settleMs);
    },
    chunk() {
      if (replaying) last = now();
    },
    allow: (input) => !(replaying && REPORT.test(input)),
    dispose() {
      clearTimeout(timer);
      timer = undefined;
      replaying = false;
    },
  };
}

export const REPLAY_FRAME = 64 * 1024;

export interface ReplayPaint {
  begin(): void;
  up(): void;
  chunk(bytes: number): void;
  end(): void;
}

// The daemon replays a terminal's history in 64 KiB frames, then sends live output. A paint
// between two of those frames draws rows the next frame scrolls away, so paints are held from the
// start of an attach until a short frame (the last of the replay), a pause, or the cap.
export function replayPaint(hold: () => void, release: () => void, quietMs = 60, maxMs = 1000): ReplayPaint {
  let held = false;
  let quiet: ReturnType<typeof setTimeout> | undefined;
  let cap: ReturnType<typeof setTimeout> | undefined;

  const end = () => {
    if (!held) return;
    held = false;
    clearTimeout(quiet);
    clearTimeout(cap);
    release();
  };
  const arm = () => {
    clearTimeout(quiet);
    quiet = setTimeout(end, quietMs);
  };

  return {
    begin() {
      clearTimeout(quiet);
      clearTimeout(cap);
      held = true;
      hold();
      cap = setTimeout(end, maxMs);
    },
    up() {
      if (held) arm();
    },
    chunk(bytes) {
      if (!held) return;
      if (bytes < REPLAY_FRAME) end();
      else arm();
    },
    end,
  };
}
