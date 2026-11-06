export const DEFAULT_SCROLLBACK_LINES = 10_000;
export const MIN_SCROLLBACK_LINES = 1_000;
export const MAX_SCROLLBACK_LINES = 50_000;

// ghostty-web 0.4.0 forwards `scrollback` to libghostty's max_scrollback, which is a byte budget,
// not a line count. A row costs 8 bytes per cell plus overhead, so 1600 bytes keeps a line of up
// to ~190 columns.
export const BYTES_PER_LINE = 1600;

export function clampScrollback(lines: number): number {
  if (!Number.isFinite(lines)) return DEFAULT_SCROLLBACK_LINES;
  return Math.min(MAX_SCROLLBACK_LINES, Math.max(MIN_SCROLLBACK_LINES, Math.round(lines)));
}

export function scrollbackBytes(lines: number): number {
  return clampScrollback(lines) * BYTES_PER_LINE;
}

export interface Viewport {
  getViewportY(): number;
  getScrollbackLength(): number;
  scrollToLine(line: number): void;
  write(data: string | Uint8Array): void;
}

export function resetViewport(term: { reset(): void; scrollToBottom(): void }): void {
  term.reset();
  term.scrollToBottom();
}

export function anchoredViewport(before: number, grown: number, length: number): number {
  if (!(before > 0)) return 0;
  return Math.max(0, Math.min(length, before + grown));
}

// ghostty-web snaps the viewport to the bottom on every write; put it back on the same content so
// a reader scrolled into history is not yanked down by each redraw. Returns whether the viewport
// is still scrolled back.
export function writeAnchored(term: Viewport, data: string | Uint8Array): boolean {
  const y = term.getViewportY();
  if (!(y > 0)) {
    term.write(data);
    return false;
  }
  const length = term.getScrollbackLength();
  term.write(data);
  const after = term.getScrollbackLength();
  const next = anchoredViewport(y, after - length, after);
  if (next > 0) term.scrollToLine(next);
  return next > 0;
}
