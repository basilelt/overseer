import { describe, expect, it } from 'vitest';
import { layout, radius, type LayoutInput } from './layout';

const n = (id: string, path = 'a.rs', size = 4): LayoutInput => ({ id, path, line: 1, size });

describe('graph layout', () => {
  it('puts definitions above their callers', () => {
    const l = layout([n('main'), n('parse'), n('lex')], [
      { from: 'main', to: 'parse' },
      { from: 'parse', to: 'lex' },
      { from: 'main', to: 'lex' },
    ]);
    const y = (id: string) => l.nodes.get(id)!.y;
    expect(y('lex')).toBeLessThan(y('parse'));
    expect(y('parse')).toBeLessThan(y('main'));
  });

  it('survives cycles and ignores unknown edges', () => {
    const l = layout([n('a'), n('b'), n('c')], [
      { from: 'a', to: 'b' },
      { from: 'b', to: 'c' },
      { from: 'c', to: 'a' },
      { from: 'a', to: 'zzz' },
    ]);
    expect(l.nodes.size).toBe(3);
    expect(new Set([...l.nodes.values()].map((p) => p.layer)).size).toBe(3);
  });

  it('keeps nodes of one file together and sizes by changed lines', () => {
    const l = layout([n('x1', 'x.ts'), n('y1', 'y.ts'), n('x2', 'x.ts'), n('y2', 'y.ts')], []);
    expect(l.layers[0]).toEqual(['x1', 'x2', 'y1', 'y2']);
    expect(radius(100)).toBeGreaterThan(radius(1));
    expect(radius(100000)).toBe(24);
  });

  it('lays out 400 nodes without overlap, deterministically and fast', () => {
    const nodes: LayoutInput[] = [];
    const edges: { from: string; to: string }[] = [];
    for (let i = 0; i < 400; i++) {
      nodes.push(n(`n${i}`, `src/f${i % 37}.ts`, (i * 7) % 60));
      if (i > 3) edges.push({ from: `n${i}`, to: `n${(i * 13) % i}` });
      if (i % 5 === 0 && i > 10) edges.push({ from: `n${i - 7}`, to: `n${i}` });
    }
    const t0 = performance.now();
    const a = layout(nodes, edges);
    const ms = performance.now() - t0;
    console.log(`layout of 400 nodes / ${edges.length} edges: ${ms.toFixed(1)} ms`);
    expect(ms).toBeLessThan(20 * 5);
    const seen = new Set<string>();
    for (const p of a.nodes.values()) {
      const k = `${p.x},${p.y}`;
      expect(seen.has(k)).toBe(false);
      seen.add(k);
    }
    const down = edges.filter((e) => a.nodes.get(e.from)!.layer > a.nodes.get(e.to)!.layer).length;
    expect(down / edges.length).toBeGreaterThan(0.8);
    const b = layout([...nodes].reverse(), [...edges].reverse());
    expect([...b.nodes.entries()].sort()).toEqual([...a.nodes.entries()].sort());
  });
});
