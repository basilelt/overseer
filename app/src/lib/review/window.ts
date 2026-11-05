export interface Chunk {
  start: number;
  end: number;
  height: number;
}

export function chunks(count: number, size: number, heightOf: (i: number) => number): Chunk[] {
  const out: Chunk[] = [];
  for (let start = 0; start < count; start += size) {
    const end = Math.min(count, start + size);
    let height = 0;
    for (let i = start; i < end; i++) height += heightOf(i);
    out.push({ start, end, height });
  }
  return out;
}

type Watch = (near: boolean) => void;

const observers = new WeakMap<Element, { io: IntersectionObserver; cbs: Map<Element, Watch> }>();
const MARGIN = '1200px 0px';

export function scrollParent(el: Element): Element | null {
  for (let p = el.parentElement; p; p = p.parentElement) {
    const o = getComputedStyle(p).overflowY;
    if ((o === 'auto' || o === 'scroll') && p.scrollHeight > p.clientHeight) return p;
  }
  return null;
}

export function watchNear(el: Element, cb: Watch): () => void {
  if (typeof IntersectionObserver === 'undefined') {
    cb(true);
    return () => {};
  }
  const root = scrollParent(el) ?? document.documentElement;
  let entry = observers.get(root);
  if (!entry) {
    const cbs = new Map<Element, Watch>();
    const io = new IntersectionObserver((es) => es.forEach((e) => cbs.get(e.target)?.(e.isIntersecting)), { root: root === document.documentElement ? null : root, rootMargin: MARGIN });
    entry = { io, cbs };
    observers.set(root, entry);
  }
  entry.cbs.set(el, cb);
  entry.io.observe(el);
  const e = entry;
  return () => {
    e.io.unobserve(el);
    e.cbs.delete(el);
  };
}
