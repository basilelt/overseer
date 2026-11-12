export interface DismissOptions {
  onclose: () => void;
  active?: boolean;
  escape?: boolean | 'stop';
  keep?: (target: Element | null) => boolean;
}

export function dismiss(node: HTMLElement, options: DismissOptions) {
  let opts = options;
  const down = (e: PointerEvent) => {
    if (opts.active === false) return;
    const target = e.target as Element | null;
    if (node.contains(target) || opts.keep?.(target)) return;
    opts.onclose();
  };
  const key = (e: KeyboardEvent) => {
    if (!opts.escape || e.key !== 'Escape') return;
    if (opts.escape === 'stop') e.stopPropagation();
    opts.onclose();
  };
  window.addEventListener('pointerdown', down);
  window.addEventListener('keydown', key);
  return {
    update(next: DismissOptions) {
      opts = next;
    },
    destroy() {
      window.removeEventListener('pointerdown', down);
      window.removeEventListener('keydown', key);
    },
  };
}
