interface AutoScrollHost {
  startAutoScroll?: (direction: number) => void;
  stopAutoScroll?: () => void;
  wasmTerm?: { getScrollbackLength(): number };
  terminal?: { getViewportY?: () => number };
}

// ghostty-web autoscrolls when a drag is within 30px of the top or bottom edge, and each tick snaps the selection end to column 0 of the first row (or the end of the last row) even when nothing can scroll, so selecting near an edge collapses to a wrong range.
export function guardAutoScroll(sm: AutoScrollHost | undefined) {
  if (!sm?.startAutoScroll || !sm.stopAutoScroll || !sm.wasmTerm || !sm.terminal?.getViewportY) return;
  const start = sm.startAutoScroll.bind(sm);
  const stop = sm.stopAutoScroll.bind(sm);
  const wasm = sm.wasmTerm;
  const term = sm.terminal;
  sm.startAutoScroll = (direction) => {
    const viewportY = Math.max(0, Math.floor(term.getViewportY!()));
    const canScroll = direction < 0 ? viewportY < wasm.getScrollbackLength() : viewportY > 0;
    if (canScroll) start(direction);
    else stop();
  };
}
