const running = new Map<string, Promise<unknown>>();

export function single<T>(key: string, run: () => Promise<T>): Promise<T> {
  const cur = running.get(key);
  if (cur) return cur as Promise<T>;
  const p = (async () => run())().finally(() => {
    if (running.get(key) === p) running.delete(key);
  });
  running.set(key, p);
  return p;
}

export const inFlight = (key: string) => running.has(key);
