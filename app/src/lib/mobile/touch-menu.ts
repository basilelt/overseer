export const TOUCH_MENU_MS = 500;

export function touchContextMenu(node: HTMLElement) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let fired = false;
  let x = 0;
  let y = 0;
  const cancel = () => clearTimeout(timer);
  const on: Record<string, (e: any) => void> = {
    pointerdown(e: PointerEvent) {
      fired = false;
      cancel();
      if (e.pointerType !== 'touch') return;
      x = e.clientX;
      y = e.clientY;
      const target = e.target;
      timer = setTimeout(() => {
        fired = true;
        target?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: x, clientY: y }));
      }, TOUCH_MENU_MS);
    },
    pointermove(e: PointerEvent) {
      if (Math.hypot(e.clientX - x, e.clientY - y) > 10) cancel();
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
