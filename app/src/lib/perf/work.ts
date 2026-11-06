import { expect } from 'vitest';

// Deterministic work counter for perf tests: counts callback calls, scanned elements and
// iterator steps of the built-in primitives, plus property reads on `counted()` inputs, so a
// quadratic regression shows up as a count ratio instead of a flaky wall-clock ratio.
// Plain index loops over arrays the code builds itself are not seen; use the primitives or
// the counted input for those.

let work = 0;
let active = false;
const bump = (n: number) => {
  if (active) work += n;
};

const proxies = new WeakMap<object, object>();

export function counted<T extends object>(target: T): T {
  let p = proxies.get(target);
  if (!p) {
    p = new Proxy(target, {
      get(t, key, receiver) {
        bump(1);
        const v = Reflect.get(t, key, receiver);
        return typeof v === 'object' && v !== null ? counted(v) : v;
      },
    });
    proxies.set(target, p);
  }
  return p as T;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Fn = (this: any, ...args: any[]) => any;
type Patch = [object, PropertyKey, Fn];

const callbackMethods = ['forEach', 'map', 'filter', 'find', 'findIndex', 'findLast', 'findLastIndex', 'some', 'every', 'reduce', 'reduceRight', 'flatMap'];
const scanMethods = ['indexOf', 'lastIndexOf', 'includes', 'join', 'concat', 'splice', 'unshift', 'reverse', 'flat', 'fill', 'slice'];
const stringMethods = ['split', 'slice', 'substring', 'indexOf', 'lastIndexOf', 'includes', 'match', 'matchAll', 'replace', 'replaceAll', 'localeCompare', 'startsWith', 'endsWith'];

function patches(): Patch[] {
  const out: Patch[] = [];
  const A = Array.prototype as unknown as Record<PropertyKey, Fn>;
  const wrapIter = (orig: Fn): Fn =>
    function (this: unknown) {
      // `arguments` + apply: a spread would go through the patched array iterator and count itself.
      const it = orig.apply(this, arguments as unknown as unknown[]) as { next: () => unknown };
      const next = it.next;
      it.next = () => (bump(1), next.call(it));
      return it;
    };
  for (const k of callbackMethods) {
    const orig = A[k];
    out.push([A, k, function (this: unknown) {
      const cb = arguments[0];
      if (typeof cb === 'function') arguments[0] = function (this: unknown) { bump(1); return cb.apply(this, arguments); };
      return orig.apply(this, arguments as unknown as unknown[]);
    }]);
  }
  const sort = A.sort;
  out.push([A, 'sort', function (this: unknown, cmp: unknown) {
    if (typeof cmp !== 'function') return sort.call(this, cmp);
    return sort.call(this, (a: unknown, b: unknown) => (bump(1), cmp(a, b)));
  }]);
  for (const k of scanMethods) {
    const orig = A[k];
    out.push([A, k, function (this: unknown[], ...a) { bump(this.length + 1); return orig.apply(this, a); }]);
  }
  for (const k of ['values', 'keys', 'entries', Symbol.iterator]) out.push([A, k, wrapIter(A[k])]);
  for (const proto of [Map.prototype, Set.prototype]) {
    const M = proto as unknown as Record<PropertyKey, Fn>;
    for (const k of ['get', 'set', 'add', 'has', 'delete']) {
      const orig = M[k];
      if (orig) out.push([M, k, function (this: unknown, ...a) { bump(1); return orig.apply(this, a); }]);
    }
    for (const k of ['values', 'keys', 'entries', Symbol.iterator]) out.push([M, k, wrapIter(M[k])]);
    const each = M.forEach;
    out.push([M, 'forEach', function (this: unknown) {
      const cb = arguments[0];
      if (typeof cb === 'function') arguments[0] = function (this: unknown) { bump(1); return cb.apply(this, arguments); };
      return each.apply(this, arguments as unknown as unknown[]);
    }]);
  }
  const S = String.prototype as unknown as Record<string, Fn>;
  for (const k of stringMethods) {
    const orig = S[k];
    out.push([S, k, function (this: string, ...a) { bump(this.length + 1); return orig.apply(this, a); }]);
  }
  for (const k of ['keys', 'values', 'entries'] as const) {
    const orig = Object[k] as unknown as Fn;
    out.push([Object, k, function (this: unknown, o: object) { const r = orig.call(this, o) as unknown[]; bump(r.length + 1); return r; }]);
  }
  return out;
}

export function countWork(run: () => void): number {
  const saved: (readonly [object, PropertyKey, PropertyDescriptor])[] = [];
  try {
    for (const [o, k, fn] of patches()) {
      const d = Object.getOwnPropertyDescriptor(o, k)!;
      Object.defineProperty(o, k, { ...d, value: fn });
      saved.push([o, k, d]);
    }
    work = 0;
    active = true;
    run();
    return work;
  } finally {
    active = false;
    for (const [o, k, d] of saved.reverse()) Object.defineProperty(o, k, d);
  }
}

export interface Growth {
  small: number;
  big: number;
  ratio: number;
}

// Each side builds its input outside the count and returns the function to measure, so
// memoized results never leak from one run into the other.
export function growth(small: () => () => void, big: () => () => void): Growth {
  const s = countWork(small());
  const b = countWork(big());
  return { small: s, big: b, ratio: b / Math.max(s, 1) };
}

export function expectLinear(small: () => () => void, big: () => () => void, factor = 10): Growth {
  const g = growth(small, big);
  expect(g.small, 'the small run did no countable work').toBeGreaterThan(0);
  expect(g.ratio, `work ${g.small} -> ${g.big} for ${factor}x the input`).toBeLessThan(factor * 2);
  return g;
}
