const cache = new WeakMap<readonly unknown[], Map<string, { n: number; map: Map<string, unknown> }>>();

// Lookup maps built once per list identity, so per-tab lookups cost O(1) instead of a scan of
// every session. Lists are treated as immutable; a length change (a test pushing into its
// fixture) rebuilds the map.
export function indexBy<T>(list: readonly T[], name: string, keyOf: (x: T) => string | null | undefined): Map<string, T> {
  let byName = cache.get(list);
  if (!byName) cache.set(list, (byName = new Map()));
  const hit = byName.get(name);
  if (hit && hit.n === list.length) return hit.map as Map<string, T>;
  const map = new Map<string, T>();
  for (const x of list) {
    const k = keyOf(x);
    if (k && !map.has(k)) map.set(k, x);
  }
  byName.set(name, { n: list.length, map });
  return map;
}
