<script lang="ts">
  import { chartPaths, type Level } from '../../system/format';

  let {
    times,
    a,
    b,
    max,
    t1,
    gapMs,
    tone = 'ok',
    height = 56,
    label,
  }: {
    times: number[];
    a: (number | null)[];
    b?: (number | null)[];
    max: number;
    t1: number;
    gapMs: number;
    tone?: Level;
    height?: number;
    label: string;
  } = $props();

  const W = 300;
  const pa = $derived(chartPaths({ values: a, times }, { width: W, height, max, t1, gapMs }));
  const pb = $derived(b ? chartPaths({ values: b, times }, { width: W, height, max, t1, gapMs }) : null);
</script>

<svg class="spark {tone}" viewBox="0 0 {W} {height}" preserveAspectRatio="none" role="img" aria-label={label} style:height="{height}px">
  <line class="base" x1="0" x2={W} y1={height - 0.5} y2={height - 0.5} />
  <path class="area" d={pa.area} />
  <path class="line" d={pa.line} />
  {#if pb}<path class="line second" d={pb.line} />{/if}
</svg>

<style>
  .spark {
    display: block;
    width: 100%;
    overflow: visible;
    --c: var(--text-2);
  }
  .spark.warn {
    --c: var(--sys-warn, #d97706);
  }
  .spark.crit {
    --c: var(--st-failed);
  }
  .base {
    stroke: var(--border);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .area {
    fill: var(--c);
    opacity: 0.12;
  }
  .line {
    fill: none;
    stroke: var(--c);
    stroke-width: 1.5;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }
  .second {
    stroke-dasharray: 3 3;
    opacity: 0.7;
  }
</style>
