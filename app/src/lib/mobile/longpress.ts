export const LONG_PRESS_MS = 500;

export function longPress(node: HTMLElement, onLong: (e: PointerEvent) => void) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let start: PointerEvent | null = null;
  let fired = false;
  const cancel = () => {
    clearTimeout(timer);
    timer = undefined;
    start = null;
  };
  const down = (e: PointerEvent) => {
    fired = false;
    cancel();
    start = e;
    timer = setTimeout(() => {
      timer = undefined;
      fired = true;
      if (start) onLong(start);
    }, LONG_PRESS_MS);
  };
  const move = (e: PointerEvent) => {
    if (start && Math.hypot(e.clientX - start.clientX, e.clientY - start.clientY) > 10) cancel();
  };
  const click = (e: MouseEvent) => {
    if (!fired) return;
    fired = false;
    e.preventDefault();
    e.stopImmediatePropagation();
  };
  const menu = (e: Event) => e.preventDefault();
  node.addEventListener('pointerdown', down);
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', cancel);
  node.addEventListener('pointercancel', cancel);
  node.addEventListener('pointerleave', cancel);
  node.addEventListener('click', click, true);
  node.addEventListener('contextmenu', menu);
  return {
    destroy() {
      cancel();
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', cancel);
      node.removeEventListener('pointercancel', cancel);
      node.removeEventListener('pointerleave', cancel);
      node.removeEventListener('click', click, true);
      node.removeEventListener('contextmenu', menu);
    },
  };
}
