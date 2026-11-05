export const REVEAL_MARGIN = '240px 0px';

export function reveal(node: Element, onshow: () => void) {
  let cb = onshow;
  if (typeof IntersectionObserver === 'undefined') {
    queueMicrotask(() => cb());
    return { update: (fn: () => void) => void (cb = fn), destroy() {} };
  }
  const io = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && cb(), { root: node.parentElement, rootMargin: REVEAL_MARGIN });
  io.observe(node);
  return {
    update: (fn: () => void) => void (cb = fn),
    destroy: () => io.disconnect(),
  };
}
