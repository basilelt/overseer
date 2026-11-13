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
