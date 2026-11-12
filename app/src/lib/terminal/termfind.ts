export interface Hit {
  row: number;
  col: number;
}

export function findHit(lines: string[], query: string, from: Hit | null, backwards: boolean): Hit | null {
  if (!query || !lines.length) return null;
  const q = query.toLowerCase();
  const hay = lines.map((l) => l.toLowerCase());
  const n = hay.length;
  if (backwards) {
    const start = from ?? { row: n - 1, col: Number.MAX_SAFE_INTEGER };
    for (let i = 0; i <= n; i++) {
      const row = (((start.row - i) % n) + n) % n;
      const col = i > 0 ? hay[row].lastIndexOf(q) : start.col > 0 ? hay[row].lastIndexOf(q, start.col - 1) : -1;
      if (col >= 0) return { row, col };
    }
    return null;
  }
  const start = from ?? { row: 0, col: -1 };
  for (let i = 0; i <= n; i++) {
    const row = (start.row + i) % n;
    const col = hay[row].indexOf(q, i === 0 ? start.col + 1 : 0);
    if (col >= 0) return { row, col };
  }
  return null;
}

export function viewportFor(row: number, scrollback: number, rows: number): { offset: number; row: number } {
  const offset = Math.max(0, Math.min(scrollback, scrollback - row + Math.floor(rows / 2)));
  return { offset, row: row - (scrollback - offset) };
}
