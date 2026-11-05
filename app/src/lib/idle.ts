type IdleHost = {
  requestIdleCallback?: (cb: () => void, opts?: { timeout: number }) => number;
  cancelIdleCallback?: (id: number) => void;
  setTimeout: (cb: () => void, ms: number) => unknown;
  clearTimeout: (id: never) => void;
};

export function whenIdle(fn: () => void, timeout = 5000, host: IdleHost = globalThis as unknown as IdleHost): () => void {
  if (host.requestIdleCallback) {
    const id = host.requestIdleCallback(fn, { timeout });
    return () => host.cancelIdleCallback?.(id);
  }
  const id = host.setTimeout(fn, Math.min(timeout, 1500));
  return () => host.clearTimeout(id as never);
}

export function preload<T>(load: () => Promise<T>, timeout = 5000, host?: IdleHost): () => Promise<T> {
  let p: Promise<T> | null = null;
  const get = () => (p ??= load().catch((e) => {
    p = null;
    throw e;
  }));
  if (host || typeof window !== 'undefined') whenIdle(() => void get().catch(() => {}), timeout, host);
  return get;
}
