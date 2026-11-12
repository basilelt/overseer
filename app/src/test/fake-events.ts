type Handler = (event: any) => void;

export function eventTarget() {
  const handlers = new Map<string, Set<Handler>>();
  return {
    addEventListener: (type: string, fn: Handler) => {
      if (!handlers.has(type)) handlers.set(type, new Set());
      handlers.get(type)!.add(fn);
    },
    removeEventListener: (type: string, fn: Handler) => void handlers.get(type)?.delete(fn),
    fire: (type: string, event?: unknown) => [...(handlers.get(type) ?? [])].forEach((fn) => fn(event)),
    has: (type: string) => (handlers.get(type)?.size ?? 0) > 0,
    count: () => [...handlers.values()].reduce((n, set) => n + set.size, 0),
    clear: () => handlers.clear(),
  };
}
