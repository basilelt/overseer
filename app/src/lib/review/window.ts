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

function scrollRoot(el: Element, memo: Map<Element, Element | null>): Element | null {
  const path: Element[] = [];
  let found: Element | null = null;
  for (let p = el.parentElement; p; p = p.parentElement) {
    const known = memo.get(p);
    if (known !== undefined) {
      found = known;
      break;
    }
    path.push(p);
    const o = getComputedStyle(p).overflowY;
    if ((o === 'auto' || o === 'scroll') && p.scrollHeight > p.clientHeight) {
      found = p;
      break;
    }
  }
  for (const p of path) memo.set(p, found);
  return found;
}

export function scrollParent(el: Element): Element | null {
  return scrollRoot(el, new Map());
}

// Mounting a diff or a chat registers many elements in one flush, each read forcing a layout
// of the freshly changed DOM; ancestors already walked in the same task are not read again.
let walked: Map<Element, Element | null> | null = null;

function cachedScrollParent(el: Element): Element | null {
  if (!walked) {
    walked = new Map();
    queueMicrotask(() => (walked = null));
  }
  return scrollRoot(el, walked);
}

export function watchNear(el: Element, cb: Watch): () => void {
  if (typeof IntersectionObserver === 'undefined') {
    cb(true);
    return () => {};
  }
  const root = cachedScrollParent(el) ?? document.documentElement;
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
