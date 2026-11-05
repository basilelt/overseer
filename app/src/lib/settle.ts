export interface Settler {
  push(key: string): void;
  cancel(): void;
}

export function settle(fn: () => void, ms: number): Settler {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending = '';
  let applied: string | null = null;
  return {
    push(key) {
      pending = key;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = null;
        if (pending === applied) return;
        applied = pending;
        fn();
      }, ms);
    },
    cancel() {
      if (timer) clearTimeout(timer);
      timer = null;
    },
  };
}
