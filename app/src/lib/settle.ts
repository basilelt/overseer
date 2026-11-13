import { OneShot } from './timer';

export interface Settler {
  push(key: string): void;
  cancel(): void;
}

export function settle(fn: () => void, ms: number): Settler {
  const timer = new OneShot();
  let pending = '';
  let applied: string | null = null;
  return {
    push(key) {
      pending = key;
      timer.set(() => {
        if (pending === applied) return;
        applied = pending;
        fn();
      }, ms);
    },
    cancel() {
      timer.clear();
    },
  };
}
