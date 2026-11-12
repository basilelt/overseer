import { LONG_PRESS_MS, pressTimer } from './gestures';

export function longPress(node: HTMLElement, onLong: (e: PointerEvent) => void) {
  const press = pressTimer(LONG_PRESS_MS);
  let fired = false;
  const down = (e: PointerEvent) => {
    fired = false;
    press.start(e.clientX, e.clientY, () => {
      fired = true;
      onLong(e);
    });
  };
  const move = (e: PointerEvent) => press.move(e.clientX, e.clientY);
  const cancel = () => press.cancel();
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
