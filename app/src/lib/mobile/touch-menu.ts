import { pressTimer, TOUCH_MENU_MS } from './gestures';

export function touchContextMenu(node: HTMLElement) {
  const press = pressTimer(TOUCH_MENU_MS);
  let fired = false;
  const cancel = () => press.cancel();
  const on: Record<string, (e: any) => void> = {
    pointerdown(e: PointerEvent) {
      fired = false;
      cancel();
      if (e.pointerType !== 'touch') return;
      const { clientX, clientY, target } = e;
      press.start(clientX, clientY, () => {
        fired = true;
        target?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX, clientY }));
      });
    },
    pointermove(e: PointerEvent) {
      press.move(e.clientX, e.clientY);
    },
    pointerup: cancel,
    pointercancel: cancel,
    contextmenu(e: Event) {
      if (!fired || !e.isTrusted) return;
      e.preventDefault();
      e.stopImmediatePropagation();
    },
    click(e: Event) {
      if (!fired) return;
      fired = false;
      e.preventDefault();
      e.stopImmediatePropagation();
    },
  };
  for (const k in on) node.addEventListener(k, on[k]!, k === 'click');
  return {
    destroy() {
      cancel();
      for (const k in on) node.removeEventListener(k, on[k]!, k === 'click');
    },
  };
}
