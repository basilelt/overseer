export interface LayoutInput {
  id: string;
  path: string;
  line: number;
  size: number;
}

export interface Placed {
  id: string;
  x: number;
  y: number;
  r: number;
  layer: number;
}

export interface Layout {
  nodes: Map<string, Placed>;
  layers: string[][];
  width: number;
  height: number;
}

export const COL = 150;
export const ROW = 96;
export const GROUP_GAP = 28;
export const PAD = 40;

export const radius = (size: number) => Math.min(24, 6 + 2.2 * Math.sqrt(Math.max(0, size)));

function acyclic(ids: string[], out: Map<string, string[]>): Map<string, string[]> {
  const state = new Map<string, number>();
  const dag = new Map<string, string[]>(ids.map((id) => [id, []]));
  for (const root of ids) {
    if (state.has(root)) continue;
    const stack: [string, number][] = [[root, 0]];
    state.set(root, 1);
    while (stack.length) {
      const top = stack[stack.length - 1];
      const next = out.get(top[0]) ?? [];
      if (top[1] >= next.length) {
        state.set(top[0], 2);
        stack.pop();
        continue;
      }
      const to = next[top[1]++];
      const s = state.get(to);
      if (s === 1) continue;
      dag.get(top[0])!.push(to);
      if (s === undefined) {
        state.set(to, 1);
        stack.push([to, 0]);
      }
    }
  }
  return dag;
}

function layering(ids: string[], dag: Map<string, string[]>): Map<string, number> {
  const layer = new Map<string, number>();
  const visit = (root: string) => {
    const stack: [string, boolean][] = [[root, false]];
    while (stack.length) {
      const [id, done] = stack.pop()!;
      if (layer.has(id) && !done) continue;
      const deps = dag.get(id) ?? [];
      if (done) {
        layer.set(id, deps.reduce((m, d) => Math.max(m, (layer.get(d) ?? 0) + 1), 0));
        continue;
      }
      stack.push([id, true]);
      for (const d of deps) if (!layer.has(d)) stack.push([d, false]);
    }
  };
  for (const id of ids) visit(id);
  return layer;
}

export function layout(input: LayoutInput[], edges: { from: string; to: string }[]): Layout {
  const byId = new Map(input.map((n) => [n.id, n]));
  const ids = [...byId.keys()].sort();
  const out = new Map<string, string[]>();
  for (const e of edges) {
    if (e.from === e.to || !byId.has(e.from) || !byId.has(e.to)) continue;
    const l = out.get(e.from) ?? [];
    if (!l.includes(e.to)) l.push(e.to);
    out.set(e.from, l);
  }
  for (const l of out.values()) l.sort();
  const dag = acyclic(ids, out);
  const layerOf = layering(ids, dag);
  const layers: string[][] = [];
  for (const id of ids) (layers[layerOf.get(id)!] ??= []).push(id);
  const cmp = (a: string, b: string) => {
    const x = byId.get(a)!;
    const y = byId.get(b)!;
    return x.path.localeCompare(y.path) || x.line - y.line || a.localeCompare(b);
  };
  for (const l of layers) l.sort(cmp);
  const up = new Map<string, string[]>();
  for (const [from, tos] of dag) for (const to of tos) up.set(to, [...(up.get(to) ?? []), from]);
  const pos = new Map<string, number>();
  const index = () => layers.forEach((l) => l.forEach((id, i) => pos.set(id, i)));
  index();
  const sweep = (l: string[], nb: Map<string, string[]>) => {
    const bary = new Map<string, number>();
    for (const id of l) {
      const n = nb.get(id) ?? [];
      bary.set(id, n.length ? n.reduce((s, x) => s + pos.get(x)!, 0) / n.length : pos.get(id)!);
    }
    const groups = new Map<string, string[]>();
    for (const id of l) {
      const p = byId.get(id)!.path;
      groups.set(p, [...(groups.get(p) ?? []), id]);
    }
    const avg = (g: string[]) => g.reduce((s, id) => s + bary.get(id)!, 0) / g.length;
    const sorted = [...groups.entries()].map(([p, g]) => ({ p, g: g.sort((a, b) => bary.get(a)! - bary.get(b)! || cmp(a, b)), b: avg(g) }));
    sorted.sort((a, b) => a.b - b.b || a.p.localeCompare(b.p));
    l.splice(0, l.length, ...sorted.flatMap((s) => s.g));
    l.forEach((id, i) => pos.set(id, i));
  };
  for (let s = 0; s < 3; s++) {
    if (s % 2 === 0) for (let i = 1; i < layers.length; i++) sweep(layers[i], dag);
    else for (let i = layers.length - 2; i >= 0; i--) sweep(layers[i], up);
  }
  const nodes = new Map<string, Placed>();
  const per = Math.max(8, Math.ceil(Math.sqrt(ids.length) * 1.5));
  let width = 0;
  let row = 0;
  layers.forEach((l, li) => {
    for (let start = 0; start < l.length; start += per) {
      let x = PAD;
      let prev = '';
      for (const id of l.slice(start, start + per)) {
        const n = byId.get(id)!;
        if (prev && prev !== n.path) x += GROUP_GAP;
        prev = n.path;
        nodes.set(id, { id, x: x + COL / 2, y: PAD + row * ROW + ROW / 2, r: radius(n.size), layer: li });
        x += COL;
      }
      width = Math.max(width, x + PAD);
      row++;
    }
  });
  return { nodes, layers, width, height: PAD * 2 + row * ROW };
}
