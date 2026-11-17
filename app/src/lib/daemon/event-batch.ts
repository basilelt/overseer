export const EVENT_BATCH_MS = 50;

export interface BatchClock {
  now(): number;
  later(fn: () => void, ms: number): unknown;
  cancel(handle: unknown): void;
}

const realClock: BatchClock = {
  now: () => performance.now(),
  later: (fn, ms) => setTimeout(fn, ms),
  cancel: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

export interface Batcher<E> {
  push(e: E): void;
  stop(): void;
}

// The desktop shell delivers each daemon event as its own task, and the page redraws after every
// task: a burst of n events cost n redraws of everything derived from the data. Hand over an event
// that arrives alone at once, and the ones that follow within the window together.
export function eventBatcher<E>(apply: (batch: E[]) => void, ms = EVENT_BATCH_MS, clock: BatchClock = realClock): Batcher<E> {
  let queue: E[] = [];
  let timer: unknown;
  let last = -Infinity;
  let stopped = false;
  const drain = () => {
    timer = undefined;
    const batch = queue;
    queue = [];
    try {
      if (batch.length) apply(batch);
    } finally {
      last = clock.now();
    }
  };
  return {
    push(e) {
      if (stopped) return;
      queue.push(e);
      if (timer !== undefined) return;
      const wait = ms - (clock.now() - last);
      if (wait <= 0) drain();
      else timer = clock.later(drain, wait);
    },
    stop() {
      stopped = true;
      queue = [];
      if (timer !== undefined) clock.cancel(timer);
      timer = undefined;
    },
  };
}
