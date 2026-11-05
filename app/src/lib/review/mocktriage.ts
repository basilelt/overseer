import { LOW, lineTags } from './triage';
import type { GraphEdge, GraphNode, ReviewFile, ReviewGraph, Tag, TagKind, Triage } from './types';

const DEF = /^\s*(?:export\s+|pub(?:\([a-z]+\))?\s+|async\s+|default\s+)*(?:fn|function|def|class|struct|enum|interface|type|impl|trait)\s+([A-Za-z_$][\w$]*)/;
const COMMENT = /^\s*(\/\/|#|\/\*|\*|<!--)/;
const norm = (s: string) => s.replace(/\s+/g, '');
const isTest = (p: string) => /(^|\/)(tests?|__tests__)\/|\.(test|spec)\.[a-z]+$|_test\.(go|py|rs)$/.test(p);

export function annotate(files: ReviewFile[]): { files: ReviewFile[]; triage: Triage } {
  type At = { path: string; hunk: number };
  const where = { add: new Map<string, At[]>(), del: new Map<string, At[]>() };
  for (const f of files)
    f.hunks.forEach((h, i) => {
      for (const l of h.lines) if (l.kind !== 'ctx' && norm(l.text).length >= 8) where[l.kind].set(norm(l.text), [...(where[l.kind].get(norm(l.text)) ?? []), { path: f.path, hunk: i }]);
    });
  const byKind: Partial<Record<TagKind, number>> = {};
  let changed = 0;
  const raw = files.map((f) => {
    let residual = 0;
    let sigs = 0;
    for (const [hi, h] of f.hunks.entries()) {
      const kinds: (Tag | null)[] = h.lines.map(() => null);
      let i = 0;
      while (i < h.lines.length) {
        if (h.lines[i].kind === 'ctx') {
          i++;
          continue;
        }
        let j = i;
        while (j < h.lines.length && h.lines[j].kind !== 'ctx') j++;
        const block = h.lines.slice(i, j);
        const side = (k: string) => block.filter((l) => l.kind === k).map((l) => norm(l.text)).join('');
        const format = side('del') === side('add');
        for (let k = i; k < j; k++) {
          const l = h.lines[k];
          const n = norm(l.text);
          const other = where[l.kind === 'add' ? 'del' : 'add'].get(n)?.find((w) => w.path !== f.path);
          const tag: Tag = { kind: 'logic', from: k, to: k + 1 };
          const prev = kinds[k - 1];
          if (format) tag.kind = 'format';
          else if (n.length < 8 && k > i && prev?.kind === 'move') Object.assign(tag, { kind: 'move', ref: prev.ref });
          else if (!n) tag.kind = 'format';
          else if (other) Object.assign(tag, { kind: 'move', ref: other });
          else if (isTest(f.path)) tag.kind = 'test';
          else if (COMMENT.test(l.text)) tag.kind = 'comment';
          else if (DEF.test(l.text)) tag.sig = true;
          kinds[k] = tag;
        }
        i = j;
      }
      const tags: Tag[] = [];
      for (const t of kinds) {
        if (!t) continue;
        const last = tags.at(-1);
        if (last && last.to === t.from && last.kind === t.kind && last.ref?.path === t.ref?.path && !!last.sig === !!t.sig) last.to = t.to;
        else tags.push({ ...t });
        byKind[t.kind] = (byKind[t.kind] ?? 0) + 1;
        changed++;
        if (!LOW.has(t.kind)) residual++;
        if (t.sig) sigs++;
      }
      f.hunks[hi] = { ...h, tags };
    }
    return { f, residual, score: residual * (1 + 0.5 * sigs) * (isTest(f.path) ? 0.2 : 1) };
  });
  const max = Math.max(0, ...raw.map((r) => r.score));
  const out = raw.map(({ f, residual, score }) => ({ ...f, residual, risk: max ? Math.round((score / max) * 100) : 0 }));
  const low = Object.entries(byKind).reduce((n, [k, v]) => n + (LOW.has(k as TagKind) ? v! : 0), 0);
  return { files: out, triage: { changed, byKind, lowRiskPct: changed ? Math.round((low / changed) * 100) : 0 } };
}

export function mockGraph(files: ReviewFile[], revision: string): ReviewGraph {
  const nodes = new Map<string, GraphNode & { seen: Record<string, number> }>();
  for (const f of files) {
    f.hunks.forEach((h, hi) => {
      const tags = lineTags(h);
      let def = DEF.exec(h.header.split('@@')[2] ?? '')?.[1];
      h.lines.forEach((l, li) => {
        if (l.kind !== 'del') def = DEF.exec(l.text)?.[1] ?? def;
        if (l.kind === 'ctx') return;
        const id = def ? `${f.path}::${def}` : f.path;
        const n = nodes.get(id) ?? { id, kind: def ? 'fn' : 'file', name: def ?? f.path.split('/').pop()!, path: f.path, line: l.new ?? l.old ?? 1, endLine: l.new ?? l.old ?? 1, hunks: [], add: 0, del: 0, tag: 'logic', risk: 0, seen: {} as Record<string, number> };
        if (!n.hunks.some((r) => r.hunk === hi)) n.hunks.push({ path: f.path, hunk: hi });
        n.endLine = Math.max(n.endLine, l.new ?? l.old ?? 1);
        if (l.kind === 'add') n.add++;
        else n.del++;
        const k = tags[li]?.kind ?? 'logic';
        n.seen[k] = (n.seen[k] ?? 0) + 1;
        n.tag = Object.entries(n.seen).sort((a, b) => b[1] - a[1])[0][0] as TagKind;
        n.risk = f.risk ?? 0;
        nodes.set(id, n);
      });
    });
  }
  const names = new Map<string, string>();
  for (const n of nodes.values()) if (n.kind !== 'file' && n.name.length >= 3) names.set(n.name, n.id);
  const edges: GraphEdge[] = [];
  const seen = new Set<string>();
  const byPath = new Map(files.map((f) => [f.path, f]));
  for (const n of nodes.values()) {
    const f = byPath.get(n.path)!;
    const text = n.hunks.flatMap((r) => f.hunks[r.hunk].lines.filter((l) => l.kind === 'add').map((l) => l.text)).join('\n');
    for (const m of text.matchAll(/([A-Za-z_$][\w$]*)\s*[(<.:]/g)) {
      const to = names.get(m[1]);
      if (!to || to === n.id || seen.has(`${n.id}>${to}`)) continue;
      seen.add(`${n.id}>${to}`);
      edges.push({ from: n.id, to, kind: 'uses' });
    }
  }
  const list: GraphNode[] = [...nodes.values()].slice(0, 400).map(({ seen: _, ...n }) => n);
  const keep = new Set(list.map((n) => n.id));
  const kept = edges.filter((e) => keep.has(e.from) && keep.has(e.to));
  const pending = new Map(list.map((n) => [n.id, kept.filter((e) => e.from === n.id).length]));
  const order: string[] = [];
  const rank = (a: GraphNode, b: GraphNode) => b.risk - a.risk || b.add + b.del - a.add - a.del || a.id.localeCompare(b.id);
  while (order.length < list.length) {
    const ready = list.filter((n) => pending.get(n.id) === 0);
    const next = (ready.length ? ready : list.filter((n) => pending.get(n.id)! > 0)).sort(rank)[0];
    order.push(next.id);
    pending.set(next.id, -1);
    for (const e of kept) if (e.to === next.id && pending.get(e.from)! > 0) pending.set(e.from, pending.get(e.from)! - 1);
  }
  return { revision, nodes: list, edges: kept, order, truncated: nodes.size > list.length };
}
