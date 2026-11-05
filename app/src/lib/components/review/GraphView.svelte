<script lang="ts">
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { t } from '../../i18n.svelte';
  import { layout, ROW } from '../../review/graph/layout';
  import type { ReviewStore } from '../../review/store.svelte';
  import { TAG_COLOR } from '../../review/triage';
  import type { GraphNode } from '../../review/types';

  let { store, phone = false, onopen, onend }: { store: ReviewStore; phone?: boolean; onopen: (n: GraphNode) => void; onend?: () => void } = $props();

  const graph = $derived(store.graph);
  const byId = $derived(new Map((graph?.nodes ?? []).map((n) => [n.id, n])));
  const order = $derived((graph?.order ?? []).map((id) => byId.get(id)).filter((n): n is GraphNode => !!n));
  const lay = $derived(graph && !phone ? layout(graph.nodes.map((n) => ({ id: n.id, path: n.path, line: n.line, size: n.add + n.del })), graph.edges) : null);
  const current = $derived(order[store.orderAt]?.id ?? null);

  let box: HTMLDivElement | undefined = $state();
  let view = $state({ x: 0, y: 0, k: 1 });
  let fitted = '';

  $effect(() => {
    if (!lay || !box || fitted === graph?.revision) return;
    fitted = graph!.revision;
    const k = Math.max(0.25, Math.min(1, (box.clientWidth - 16) / lay.width, (box.clientHeight - 16) / lay.height));
    view = { x: Math.max(8, (box.clientWidth - lay.width * k) / 2), y: 8, k };
  });

  function wheel(e: WheelEvent) {
    e.preventDefault();
    const r = box!.getBoundingClientRect();
    const k = Math.max(0.15, Math.min(3, view.k * Math.exp(-e.deltaY * 0.0015)));
    const px = e.clientX - r.left;
    const py = e.clientY - r.top;
    view = { x: px - ((px - view.x) * k) / view.k, y: py - ((py - view.y) * k) / view.k, k };
  }

  let drag: { x: number; y: number; vx: number; vy: number; moved: boolean } | null = null;
  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
    if (drag.moved) view = { ...view, x: drag.vx + dx, y: drag.vy + dy };
  }
  function up(e: PointerEvent) {
    const d = drag;
    drag = null;
    if (d?.moved) return;
    const id = (e.target as Element).closest<SVGElement>('[data-id]')?.dataset.id;
    const n = id ? byId.get(id) : undefined;
    if (n) open(n);
  }

  function open(n: GraphNode) {
    store.orderAt = order.indexOf(n);
    onopen(n);
  }

  function next() {
    if (onend && store.orderAt >= order.length - 1) return onend();
    const n = order[Math.min(order.length - 1, store.orderAt + 1)];
    if (n) open(n);
  }

  function edgePath(from: string, to: string): string {
    const a = lay!.nodes.get(from);
    const b = lay!.nodes.get(to);
    if (!a || !b) return '';
    if (a.y === b.y) {
      const lift = ROW * 0.45;
      return `M${a.x},${a.y - a.r} C${a.x},${a.y - lift} ${b.x},${b.y - lift} ${b.x},${b.y - b.r}`;
    }
    const [y1, y2] = a.y > b.y ? [a.y - a.r, b.y + b.r + LABEL] : [a.y + a.r + LABEL, b.y - b.r];
    const m = (y1 + y2) / 2;
    return `M${a.x},${y1} C${a.x},${m} ${b.x},${m} ${b.x},${y2}`;
  }

  const LABEL = 30;
  const short = (s: string, n: number) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);
  const base = (p: string) => p.split('/').pop() ?? p;
</script>

<div class="gv" class:phone>
  {#if !graph}
    <div class="state faint">
      {#if store.graphError}<span class="err">{store.graphError}</span>{:else}<LoaderCircle size={14} class="spin" />{t('common.loading')}{/if}
    </div>
  {:else if !graph.nodes.length}
    <div class="state faint">{t('review.graphEmpty')}</div>
  {:else}
    {#if lay}
      <div class="canvas" bind:this={box} onwheel={wheel} onpointerdown={down} onpointermove={move} onpointerup={up} role="presentation">
        <svg width="100%" height="100%" aria-label={t('review.graph')}>
          <defs>
            <marker id="gv-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M0,0 L8,4 L0,8 z" class="arrow" />
            </marker>
          </defs>
          <g transform="translate({view.x},{view.y}) scale({view.k})">
            {#each graph.edges as e (e.from + '>' + e.to + e.kind)}
              <path class="edge {e.kind}" class:hot={e.from === current || e.to === current} d={edgePath(e.from, e.to)} marker-end="url(#gv-arrow)" />
            {/each}
            {#each graph.nodes as n (n.id)}
              {@const p = lay.nodes.get(n.id)}
              {#if p}
                <g class="node" class:cur={n.id === current} data-id={n.id} transform="translate({p.x},{p.y})" style:--c={TAG_COLOR[n.tag]}>
                  <title>{n.path}:{n.line} · {t(`review.tag.${n.tag}`)} · {t('review.risk', { n: n.risk })}</title>
                  <circle r={p.r} />
                  <text class="nm" y={p.r + 13}>{short(n.name, 20)}</text>
                  {#if n.kind !== 'file'}<text class="fp" y={p.r + 25}>{short(base(n.path), 22)}</text>{/if}
                </g>
              {/if}
            {/each}
          </g>
        </svg>
        <div class="hint faint">{t('review.graphHint')}</div>
      </div>
    {/if}
    <aside class="order">
      <div class="otitle">
        <span>{t('review.order')}</span>
        <button class="next" onclick={next} disabled={!onend && store.orderAt >= order.length - 1}>{t('review.next')}<ChevronRight size={12} /></button>
      </div>
      {#if graph.truncated}<p class="note faint">{t('review.graphTruncated')}</p>{/if}
      <ol>
        {#each order as n, i (n.id)}
          <li>
            <button class="item" class:cur={i === store.orderAt} onclick={() => open(n)} title={`${n.path}:${n.line}`}>
              <span class="dot" style:background={TAG_COLOR[n.tag]}></span>
              <span class="nm mono">{n.name}</span>
              <span class="faint fp">{n.kind === 'file' ? '' : base(n.path)}</span>
              <span class="faint ch">+{n.add} −{n.del}</span>
            </button>
          </li>
        {/each}
      </ol>
    </aside>
  {/if}
</div>

<style>
  .gv {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }
  .err {
    color: var(--st-failed);
  }
  .canvas {
    flex: 1;
    min-width: 0;
    position: relative;
    overflow: hidden;
    cursor: grab;
    touch-action: none;
    background: var(--bg);
  }
  svg {
    display: block;
    user-select: none;
    -webkit-user-select: none;
  }
  .edge {
    fill: none;
    stroke: var(--border-strong);
    stroke-width: 1.3;
  }
  .edge.imports {
    stroke-dasharray: 4 3;
  }
  .edge.hot {
    stroke: var(--accent);
    stroke-width: 2;
  }
  .arrow {
    fill: var(--text-3);
  }
  .node {
    cursor: pointer;
  }
  .node circle {
    fill: color-mix(in srgb, var(--c) 28%, var(--bg));
    stroke: var(--c);
    stroke-width: 1.6;
  }
  .node:hover circle {
    fill: color-mix(in srgb, var(--c) 45%, var(--bg));
  }
  .node.cur circle {
    stroke: var(--accent);
    stroke-width: 3;
  }
  .node text {
    text-anchor: middle;
    paint-order: stroke;
    stroke: var(--bg);
    stroke-width: 3px;
    stroke-linejoin: round;
    font-family: var(--mono);
    pointer-events: none;
  }
  .node .nm {
    font-size: 11px;
    fill: var(--text);
  }
  .node .fp {
    font-size: 9.5px;
    fill: var(--text-3);
  }
  .hint {
    position: absolute;
    left: 12px;
    bottom: 10px;
    font-size: 11.5px;
    pointer-events: none;
  }
  .order {
    width: 280px;
    flex: none;
    overflow: auto;
    border-left: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .phone .order {
    width: auto;
    flex: 1;
    border-left: none;
  }
  .otitle {
    position: sticky;
    top: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 10px 6px 14px;
    background: var(--bg-subtle);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-3);
    text-transform: uppercase;
  }
  .next {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 24px;
    padding: 0 8px;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--accent-fg);
    font-size: 11.5px;
    font-weight: 500;
    text-transform: none;
    letter-spacing: 0;
  }
  .next:disabled {
    opacity: 0.5;
  }
  .note {
    margin: 0;
    padding: 0 14px 6px;
    font-size: 11.5px;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0 6px 12px;
    counter-reset: o;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border-radius: var(--radius);
    font-size: 12px;
    text-align: left;
    color: var(--text-2);
  }
  .phone .item {
    height: 44px;
  }
  .item::before {
    counter-increment: o;
    content: counter(o);
    width: 20px;
    flex: none;
    font-size: 10.5px;
    color: var(--text-3);
    text-align: right;
  }
  .item:hover {
    background: var(--surface-hover);
  }
  .item.cur {
    background: var(--surface-active);
    color: var(--text);
  }
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
  }
  .item .nm {
    color: var(--text);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item .fp {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }
  .item .ch {
    flex: none;
    font-family: var(--mono);
    font-size: 10.5px;
  }
</style>
