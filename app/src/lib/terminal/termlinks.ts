import { findPaths } from '../files';

export interface CellLine {
  length: number;
  getCell(x: number): { getCodepoint(): number } | undefined;
}

export interface PathRange {
  path: string;
  x0: number;
  x1: number;
}

export function pathRanges(line: CellLine): PathRange[] {
  let text = '';
  const cols: number[] = [];
  for (let x = 0; x < line.length; x++) {
    const c = line.getCell(x)?.getCodepoint() ?? 0;
    const ch = c < 32 ? ' ' : String.fromCodePoint(c);
    for (let k = 0; k < ch.length; k++) cols.push(x);
    text += ch;
  }
  return findPaths(text).map((m) => ({ path: m.path, x0: cols[m.start], x1: cols[m.end - 1] }));
}

type Opener = Pick<Window, 'open' | 'addEventListener' | 'removeEventListener'>;

export function routeTermUrls(w: Opener, route: (url: string, termId: string, shift: boolean) => boolean, now: () => number = Date.now): () => void {
  let from: { termId: string; at: number; shift: boolean } | null = null;
  const open = w.open;
  const down = (e: Event) => {
    const id = (e.target as Element | null)?.closest?.('[data-term-id]')?.getAttribute('data-term-id');
    from = id ? { termId: id, at: now(), shift: !!(e as MouseEvent).shiftKey } : null;
  };
  w.addEventListener('pointerdown', down, true);
  w.open = (url?: string | URL, target?: string, features?: string) => {
    const u = String(url ?? '');
    const src = from;
    if (src && now() - src.at < 1500 && /^https?:\/\//i.test(u) && route(u, src.termId, src.shift)) {
      from = null;
      return null;
    }
    return open.call(w, url, target, features);
  };
  return () => {
    w.removeEventListener('pointerdown', down, true);
    w.open = open;
  };
}
