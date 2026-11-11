import type { ReviewApi } from './api';
import type { MapFile, MapLink, MapRegion, ReviewMap, ReviewTarget } from './types';

export const SIGNALS = ['logic', 'reach', 'contract', 'struggle', 'untested', 'unseen'] as const;
export type Signal = (typeof SIGNALS)[number];
export type Indicator = 'priority' | Signal;
export const INDICATORS: Indicator[] = ['priority', ...SIGNALS];
export const WEIGHTS: Record<Signal, number> = { logic: 1.2, struggle: 1.1, reach: 1, contract: 1, untested: 0.9, unseen: 0.6 };

export interface Scored {
  id: string;
  w: number;
  region: MapRegion;
  file: MapFile;
  raw: Record<Signal, number>;
  score: Record<Indicator, number>;
  inbound: MapLink[];
  outbound: MapLink[];
}

export const fid = (w: number, path: string) => `${w}\n${path}`;

export function heatLevel(t: number): number {
  return t <= 0 ? -1 : Math.min(5, Math.floor(t * 6));
}

export function heat(t: number): string {
  const k = heatLevel(t);
  return k < 0 ? 'var(--surface-active)' : `var(--heat-${k})`;
}

export function heatInk(t: number): string {
  const k = heatLevel(t);
  return k < 0 ? 'var(--text-2)' : `var(--heat-ink-${k})`;
}
export const endId = (e: { w: number; path: string }) => fid(e.w, e.path);

export function score(m: ReviewMap): Scored[] {
  const inbound = new Map<string, MapLink[]>();
  const outbound = new Map<string, MapLink[]>();
  for (const l of m.links) {
    const to = endId(l.to), from = endId(l.from);
    (inbound.get(to) ?? inbound.set(to, []).get(to)!).push(l);
    (outbound.get(from) ?? outbound.set(from, []).get(from)!).push(l);
  }
  const testFiles = new Set<string>();
  m.regions.forEach((r, w) => r.changed.forEach((f) => f.test && testFiles.add(fid(w, f.path))));
  const out: Scored[] = [];
  m.regions.forEach((region, w) => {
    for (const file of region.changed) {
      const id = fid(w, file.path);
      const ins = inbound.get(id) ?? [];
      const tested = ins.some((l) => testFiles.has(endId(l.from)));
      const raw: Record<Signal, number> = {
        logic: file.logic,
        reach: ins.reduce((s, l) => s + (l.cross ? 2 : 1), 0),
        contract: file.sig > 0 ? file.sig + (ins.some((l) => l.cross) ? 1 : 0) : 0,
        struggle: file.edits + 2 * file.fails,
        untested: !file.test && file.logic > 0 && !tested ? file.logic : 0,
        unseen: (file.viewed ? 0 : file.logic) + 3 * file.threads,
      };
      out.push({ id, w, region, file, raw, score: { priority: 0, ...raw }, inbound: ins, outbound: outbound.get(id) ?? [] });
    }
  });
  const total = SIGNALS.reduce((s, k) => s + WEIGHTS[k], 0);
  for (const k of SIGNALS) {
    const max = Math.max(0, ...out.map((s) => s.raw[k]));
    for (const s of out) s.score[k] = max > 0 ? s.raw[k] / max : 0;
  }
  for (const s of out) s.score.priority = SIGNALS.reduce((a, k) => a + WEIGHTS[k] * s.score[k], 0) / total;
  const top = Math.max(0, ...out.map((s) => s.score.priority));
  for (const s of out) s.score.priority = top > 0 ? s.score.priority / top : 0;
  return out;
}

export function ranking(list: Scored[], ind: Indicator): Scored[] {
  return list.slice().sort((a, b) => b.score[ind] - a.score[ind] || a.w - b.w || a.file.path.localeCompare(b.file.path));
}

export type Reason = { key: string; params?: Record<string, string | number> };

export function reasons(s: Scored): Reason[] {
  const f = s.file, out: Reason[] = [];
  if (f.logic) out.push({ key: f.noise ? 'map.why.logicNoise' : 'map.why.logic', params: { n: f.logic, noise: f.noise } });
  else if (f.noise) out.push({ key: 'map.why.noiseOnly', params: { n: f.noise } });
  const cross = s.inbound.filter((l) => l.cross).length;
  if (s.inbound.length) out.push({ key: cross ? 'map.why.usedCross' : 'map.why.used', params: { n: s.inbound.length, x: cross } });
  if (f.sig) out.push({ key: 'map.why.contract' });
  if (f.edits > 1 || f.fails) out.push({ key: f.fails ? 'map.why.struggleFails' : 'map.why.struggle', params: { n: f.edits, f: f.fails } });
  if (s.raw.untested) out.push({ key: 'map.why.untested' });
  if (!f.viewed && f.logic) out.push({ key: 'map.why.unseen' });
  if (f.threads) out.push({ key: 'map.why.threads', params: { n: f.threads } });
  return out;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function squarify<T extends { size: number }>(items: T[], box: Rect): (Rect & { item: T })[] {
  const list = items.filter((i) => i.size > 0).sort((a, b) => b.size - a.size);
  const out: (Rect & { item: T })[] = [];
  const total = list.reduce((s, i) => s + i.size, 0);
  let { x, y, w, h } = box;
  if (!total || w <= 0 || h <= 0) return out;
  const scale = (w * h) / total;
  const worst = (s: number, mn: number, mx: number, side: number) => Math.max((side * side * mx) / (s * s), (s * s) / (side * side * mn));
  const lay = (row: T[], s: number) => {
    if (w >= h) {
      const cw = s / h;
      let cy = y;
      for (const item of row) {
        const ch = (item.size * scale) / cw;
        out.push({ x, y: cy, w: cw, h: ch, item });
        cy += ch;
      }
      x += cw;
      w -= cw;
    } else {
      const ch = s / w;
      let cx = x;
      for (const item of row) {
        const cw = (item.size * scale) / ch;
        out.push({ x: cx, y, w: cw, h: ch, item });
        cx += cw;
      }
      y += ch;
      h -= ch;
    }
  };
  let row: T[] = [];
  let sum = 0, mn = Infinity, mx = 0;
  for (const item of list) {
    const a = item.size * scale, side = Math.min(w, h);
    if (!row.length || worst(sum + a, Math.min(mn, a), Math.max(mx, a), side) <= worst(sum, mn, mx, side)) {
      row.push(item);
      sum += a;
      mn = Math.min(mn, a);
      mx = Math.max(mx, a);
    } else {
      lay(row, sum);
      row = [item];
      sum = a;
      mn = a;
      mx = a;
    }
  }
  if (row.length) lay(row, sum);
  return out;
}

interface Dir {
  name: string;
  dirs: Map<string, Dir>;
  files: { path: string; size: number; changed: boolean }[];
  size: number;
  hot: boolean;
}

export interface Cell extends Rect {
  id: string;
  w0: number;
  path: string;
  label: string;
  changed: boolean;
  block: boolean;
  count: number;
}

export interface Box extends Rect {
  label: string;
}

export interface Layout {
  regions: (Rect & { w0: number; region: MapRegion })[];
  dirs: Box[];
  cells: Cell[];
  work: number;
}

const OTHERS = 40;

function treeOf(r: MapRegion): Dir {
  const root: Dir = { name: '', dirs: new Map(), files: [], size: 0, hot: false };
  const add = (path: string, size: number, changed: boolean) => {
    const parts = path.split('/');
    let d = root;
    for (const p of parts.slice(0, -1)) {
      let n = d.dirs.get(p);
      if (!n) d.dirs.set(p, (n = { name: p, dirs: new Map(), files: [], size: 0, hot: false }));
      d = n;
    }
    d.files.push({ path, size: Math.max(1, size), changed });
  };
  for (const [p, s] of r.tree) add(p, s, false);
  for (const f of r.changed) add(f.path, f.size, true);
  const sum = (d: Dir): void => {
    for (const k of d.dirs.values()) sum(k);
    d.size = d.files.reduce((s, f) => s + f.size, 0) + [...d.dirs.values()].reduce((s, k) => s + k.size, 0);
    d.hot = d.files.some((f) => f.changed) || [...d.dirs.values()].some((k) => k.hot);
  };
  sum(root);
  return root;
}

export function layout(m: ReviewMap, box: Rect, pad = 6): Layout {
  const out: Layout = { regions: [], dirs: [], cells: [], work: 0 };
  const trees = m.regions.map(treeOf);
  const total = trees.reduce((s, t) => s + t.size, 0) || 1;
  const floor = total * 0.08;
  const regs = squarify(
    trees.map((t, w) => ({ size: Math.max(t.size, floor), w })),
    { x: box.x + pad, y: box.y + pad, w: box.w - pad * 2, h: box.h - pad * 2 },
  );
  for (const r of regs) {
    const w0 = r.item.w, region = m.regions[w0];
    out.regions.push({ x: r.x + 3, y: r.y + 3, w: r.w - 6, h: r.h - 6, w0, region });
    walk(trees[w0], { x: r.x + 9, y: r.y + 26, w: r.w - 18, h: r.h - 34 }, w0, '', out);
  }
  return out;
}

function walk(d: Dir, box: Rect, w0: number, prefix: string, out: Layout) {
  if (box.w < 2 || box.h < 2) return;
  type Item = { size: number; dir?: Dir; file?: Dir['files'][number]; rest?: Dir['files'] };
  const cold = d.files.filter((f) => !f.changed);
  const keep = cold.length > OTHERS ? cold.sort((a, b) => b.size - a.size).slice(0, OTHERS) : cold;
  const rest = cold.length > OTHERS ? cold.slice(OTHERS) : [];
  const items: Item[] = [
    ...[...d.dirs.values()].map((k) => ({ size: k.size, dir: k })),
    ...d.files.filter((f) => f.changed).map((f) => ({ size: f.size, file: f })),
    ...keep.map((f) => ({ size: f.size, file: f })),
  ];
  if (rest.length) items.push({ size: rest.reduce((s, f) => s + f.size, 0), rest });
  out.work += items.length;
  for (const r of squarify(items, box)) {
    const it = r.item;
    if (it.dir) {
      const path = prefix + it.dir.name;
      if (!it.dir.hot) {
        out.cells.push({ x: r.x, y: r.y, w: r.w, h: r.h, id: fid(w0, path + '/'), w0, path: path + '/', label: it.dir.name + '/', changed: false, block: true, count: countFiles(it.dir) });
        continue;
      }
      const head = r.w > 80 && r.h > 44 ? 14 : 2;
      out.dirs.push({ x: r.x, y: r.y, w: r.w, h: r.h, label: head > 2 ? it.dir.name : '' });
      walk(it.dir, { x: r.x + 2, y: r.y + head, w: r.w - 4, h: r.h - head - 2 }, w0, path + '/', out);
    } else if (it.file) {
      const f = it.file;
      out.cells.push({ x: r.x, y: r.y, w: r.w, h: r.h, id: fid(w0, f.path), w0, path: f.path, label: f.path.split('/').pop() ?? f.path, changed: f.changed, block: false, count: 1 });
    } else if (it.rest) {
      out.cells.push({ x: r.x, y: r.y, w: r.w, h: r.h, id: fid(w0, prefix + '…'), w0, path: prefix, label: '…', changed: false, block: true, count: it.rest.length });
    }
  }
}

function countFiles(d: Dir): number {
  return d.files.length + [...d.dirs.values()].reduce((s, k) => s + countFiles(k), 0);
}

export interface Band {
  id: string;
  share: number;
  heat: number;
}

export function band(list: Scored[], order: string[], ind: Indicator): Band[] {
  const by = new Map(list.map((s) => [s.id, s]));
  const rows = order.map((id) => by.get(id)).filter((s): s is Scored => !!s);
  const total = rows.reduce((a, s) => a + Math.max(1, s.file.add + s.file.del), 0) || 1;
  return rows.map((s) => ({ id: s.id, share: Math.max(1, s.file.add + s.file.del) / total, heat: s.score[ind] }));
}

export function step(rank: Scored[], current: string | null, dir: 1 | -1): Scored | undefined {
  if (!rank.length) return undefined;
  const i = rank.findIndex((s) => s.id === current);
  if (i < 0) return dir > 0 ? rank[0] : rank[rank.length - 1];
  return rank[(i + dir + rank.length) % rank.length];
}

const VIEW_KEY = 'overseer.review.view';

export function savedView(): 'list' | 'map' {
  try {
    const v = globalThis.localStorage?.getItem(VIEW_KEY);
    return v === 'map' || v === 'graph' ? 'map' : 'list';
  } catch {
    return 'list';
  }
}

export function saveView(v: 'list' | 'map') {
  try {
    globalThis.localStorage?.setItem(VIEW_KEY, v);
  } catch {
    return;
  }
}

export class ReviewMapStore {
  data = $state.raw<ReviewMap | null>(null);
  scored = $state.raw<Scored[]>([]);
  error = $state('');
  loading = $state(false);
  private want = '';
  private seq = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(private api: () => ReviewApi | null) {}

  request(key: string, target: ReviewTarget, also: string[], delay = 300) {
    if (key === this.want) return;
    this.want = key;
    clearTimeout(this.timer);
    if (!this.data || delay <= 0) {
      this.load(target, also);
      return;
    }
    this.timer = setTimeout(() => this.load(target, also), delay);
  }

  async load(target: ReviewTarget, also: string[]) {
    const api = this.api();
    if (!api) return;
    const n = ++this.seq;
    this.loading = true;
    try {
      const m = await api.map(target, also);
      if (n !== this.seq) return;
      this.data = m;
      this.scored = score(m);
      this.error = '';
    } catch (e) {
      if (n !== this.seq) return;
      this.error = String((e as Error)?.message ?? e);
      this.want = '';
    } finally {
      if (n === this.seq) this.loading = false;
    }
  }

  stop() {
    clearTimeout(this.timer);
    this.seq++;
  }
}
