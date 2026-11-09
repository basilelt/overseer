<script lang="ts">
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { t, type Key } from '../../i18n.svelte';
  import { INDICATORS, endId, heat, heatInk, layout, ranking, reasons, type Indicator, type ReviewMapStore, type Scored } from '../../review/map.svelte';
  import type { MapLink } from '../../review/types';

  let {
    map,
    ind = $bindable('priority'),
    phone = false,
    current = null,
    onopen,
  }: { map: ReviewMapStore; ind?: Indicator; phone?: boolean; current?: string | null; onopen: (w: number, path: string) => void } = $props();

  let width = $state(0);
  let height = $state(0);
  let hover = $state<string | null>(null);
  let pinned = $state<string | null>(null);
  let all = $state(false);

  const data = $derived(map.data);
  const scored = $derived(map.scored);
  const byId = $derived(new Map(scored.map((s) => [s.id, s])));
  const rank = $derived(ranking(scored, ind));
  const geo = $derived(data && width > 40 && height > 40 ? layout(data, { x: 0, y: 0, w: width, h: height }) : null);
  const centers = $derived(new Map((geo?.cells ?? []).map((c) => [c.id, [c.x + c.w / 2, c.y + c.h / 2] as const])));
  const selected = $derived(pinned ?? current ?? rank[0]?.id ?? null);
  const focus = $derived(hover ?? (pinned || current ? selected : null));
  const sel = $derived(selected ? byId.get(selected) : undefined);
  const near = $derived(new Set(focus ? (data?.links ?? []).flatMap((l) => (endId(l.from) === focus ? [endId(l.to)] : endId(l.to) === focus ? [endId(l.from)] : [])) : []));

  $effect(() => {
    if (current) pinned = null;
  });

  const label = (s: Scored) => s.file.path.split('/').pop() ?? s.file.path;
  const pct = (v: number) => Math.round(v * 100);
  const regionName = (w: number) => {
    const r = data?.regions[w];
    if (!r) return '';
    return r.parent !== null && data?.regions[r.parent] ? `${data.regions[r.parent].name} › ${r.name}` : r.name;
  };

  function curve(l: MapLink): string | null {
    const a = centers.get(endId(l.from)), b = centers.get(endId(l.to));
    if (!a || !b) return null;
    const [x1, y1] = a, [x2, y2] = b;
    const dx = x2 - x1, dy = y2 - y1, d = Math.hypot(dx, dy) || 1, k = Math.min(120, d * 0.32);
    return `M${x1},${y1} Q${(x1 + x2) / 2 - (dy / d) * k},${(y1 + y2) / 2 + (dx / d) * k} ${x2},${y2}`;
  }

  const linkColor = (l: MapLink, from: boolean) => (l.cross ? 'var(--link-cross)' : from ? 'var(--link-out)' : 'var(--link-in)');
  const focused = $derived(focus ? (data?.links ?? []).filter((l) => endId(l.from) === focus || endId(l.to) === focus) : []);

  function pick(id: string) {
    pinned = pinned === id ? null : id;
  }

  function open(s: Scored | undefined) {
    if (s) onopen(s.w, s.file.path);
  }
</script>

<section class="map" class:phone aria-label={t('review.map')}>
  <div class="inds" role="radiogroup" aria-label={t('map.indicator')}>
    {#each INDICATORS as k (k)}
      <button role="radio" aria-checked={ind === k} class:on={ind === k} title={t(`map.help.${k}` as Key)} onclick={() => (ind = k)}>{t(`map.ind.${k}` as Key)}</button>
    {/each}
  </div>

  {#if map.error && !data}
    <p class="state err" role="alert">{map.error}</p>
  {:else if !data}
    <p class="state faint"><LoaderCircle size={14} class="spin" />{t('map.loading')}</p>
  {:else if !scored.length}
    <p class="state faint">{t('map.empty')}</p>
  {:else}
    <div class="body">
      <div class="stage">
        <div class="legend">
          <span class="key"><i style:border-color="var(--link-out)"></i>{t('map.uses')}</span>
          <span class="key"><i style:border-color="var(--link-in)"></i>{t('map.usedBy')}</span>
          <span class="key"><i class="dash" style:border-color="var(--link-cross)"></i>{t('map.otherRepo')}</span>
          <label class="all"><input type="checkbox" bind:checked={all} />{t('map.allLinks')}</label>
          <span class="spacer"></span>
          <span class="ramp" aria-hidden="true">{t('map.low')}<b></b>{t('map.high')}</span>
        </div>
        <div class="canvas" bind:clientWidth={width} bind:clientHeight={height} class:focus={!!focus}>
          {#if geo}
            {#each geo.regions as r (r.w0)}
              <div class="region" style:left="{r.x}px" style:top="{r.y}px" style:width="{r.w}px" style:height="{r.h}px">
                <span class="rlab">{regionName(r.w0)}<span class="mono faint">{r.region.branch ?? ''}{r.region.changed.length ? ` · ${t('map.changedN', { n: r.region.changed.length })}` : ` · ${t('map.clean')}`}</span></span>
                {#if r.region.error}<span class="rerr">{r.region.error}</span>{/if}
              </div>
            {/each}
            {#each geo.dirs as d, i (i)}
              <div class="dir" style:left="{d.x}px" style:top="{d.y}px" style:width="{d.w}px" style:height="{d.h}px">{#if d.label}<i>{d.label}</i>{/if}</div>
            {/each}
            {#each geo.cells as c (c.id)}
              {@const s = c.changed ? byId.get(c.id) : undefined}
              {#if s}
                {@const v = s.score[ind]}
                <button
                  class="cell hot"
                  class:sel={selected === c.id && (pinned || current)}
                  class:near={near.has(c.id)}
                  style:left="{c.x}px"
                  style:top="{c.y}px"
                  style:width="{c.w}px"
                  style:height="{c.h}px"
                  style:background={heat(v)}
                  style:color={heatInk(v)}
                  title={`${regionName(c.w0)}/${c.path} · ${t(`map.ind.${ind}` as Key)} ${pct(v)}`}
                  aria-label={`${c.path}, ${t(`map.ind.${ind}` as Key)} ${pct(v)}`}
                  onpointerenter={() => (hover = c.id)}
                  onpointerleave={() => (hover = null)}
                  onclick={() => pick(c.id)}
                  ondblclick={() => open(s)}
                >{#if c.w > 46 && c.h > 15}<em>{c.label}</em>{/if}</button>
              {:else}
                <div class="cell" class:block={c.block} style:left="{c.x}px" style:top="{c.y}px" style:width="{c.w}px" style:height="{c.h}px" title={c.block ? t('map.block', { n: c.count, path: c.path }) : c.path}>{#if c.block && c.w > 60 && c.h > 15}<em>{c.label}</em>{/if}</div>
              {/if}
            {/each}
            <svg class="links" width={width} height={height} aria-hidden="true">
              {#if all}
                {#each data.links as l, i (i)}
                  {@const d = curve(l)}
                  {#if d}<path class="faint" {d} stroke={l.cross ? 'var(--link-cross)' : 'var(--text-3)'} stroke-dasharray={l.cross ? '5 4' : undefined} />{/if}
                {/each}
              {/if}
              {#each focused as l, i (i)}
                {@const d = curve(l)}
                {@const out = endId(l.from) === focus}
                {#if d}
                  <path {d} stroke={linkColor(l, out)} stroke-dasharray={l.cross ? '6 4' : undefined} stroke-width={l.probable ? 1.3 : 1.8} />
                  {@const end = centers.get(endId(l.to))}
                  {#if end}<circle cx={end[0]} cy={end[1]} r="3.5" fill={linkColor(l, out)} />{/if}
                {/if}
              {/each}
            </svg>
          {/if}
        </div>
      </div>

      <aside class="side">
        <div class="sec">
          <span class="eyebrow">{t('map.order', { ind: t(`map.ind.${ind}` as Key).toLowerCase() })}</span>
          <ol class="rank">
            {#each rank as s, i (s.id)}
              {@const v = s.score[ind]}
              <li>
                <button class:on={selected === s.id} onpointerenter={() => (hover = s.id)} onpointerleave={() => (hover = null)} onclick={() => pick(s.id)} ondblclick={() => open(s)}>
                  <span class="n">{i + 1}</span>
                  <span class="p"><b class="mono">{label(s)}</b><span class="faint mono">{regionName(s.w)}/{s.file.path}</span></span>
                  <span class="bar"><i style:width="{Math.max(2, pct(v))}%" style:background={heat(Math.max(v, 0.01))}></i></span>
                  <span class="v mono">{pct(v)}</span>
                </button>
              </li>
            {/each}
          </ol>
        </div>
        {#if sel}
          <div class="sec detail" aria-live="polite">
            <span class="eyebrow">{regionName(sel.w)}</span>
            <div class="ftitle mono">{label(sel)}</div>
            <div class="fsub mono faint">{sel.file.path}</div>
            <div class="sigs">
              {#each INDICATORS as k (k)}
                <div class="sig" class:on={k === ind}><span>{t(`map.ind.${k}` as Key)}</span><span class="bar"><i style:width="{Math.max(1.5, pct(sel.score[k]))}%" style:background={heat(Math.max(sel.score[k], 0.01))}></i></span><span class="v mono">{pct(sel.score[k])}</span></div>
              {/each}
            </div>
            <ul class="why">
              {#each reasons(sel) as r (r.key)}<li>{t(r.key as Key, r.params)}</li>{/each}
            </ul>
            {#if sel.inbound.length || sel.outbound.length}
              <span class="eyebrow">{t('map.links')}</span>
              <div class="lns">
                {#each [...sel.outbound.map((l) => ({ l, out: true })), ...sel.inbound.map((l) => ({ l, out: false }))] as x, i (i)}
                  {@const other = x.out ? x.l.to : x.l.from}
                  <button onclick={() => byId.has(endId(other)) && pick(endId(other))} onpointerenter={() => (hover = endId(other))} onpointerleave={() => (hover = null)}>
                    <i class:dash={x.l.cross} style:border-color={linkColor(x.l, x.out)}></i>
                    <span class="mono">{regionName(other.w)}/{other.path}</span>
                    <small>{x.out ? t('map.uses') : t('map.usedBy')}{x.l.probable ? ` · ${t('map.probable')}` : ''}</small>
                  </button>
                {/each}
              </div>
            {/if}
            <button class="btn primary openbtn" onclick={() => open(sel)}>{t('map.open')}<ArrowRight size={13} /></button>
          </div>
        {/if}
      </aside>
    </div>
    {#if data.regions.some((r) => r.truncated)}<p class="note faint">{t('map.truncated')}</p>{/if}
  {/if}
</section>

<style>
  .map {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
  .inds {
    display: flex;
    gap: 4px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    flex-wrap: wrap;
    flex: none;
  }
  .inds button {
    flex: none;
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 12px;
    white-space: nowrap;
  }
  .inds button:hover {
    color: var(--text);
    border-color: var(--border-strong);
  }
  .inds button.on {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--text);
    font-weight: 500;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 24px;
    margin: 0;
  }
  .err {
    color: var(--st-failed);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-wrap: wrap;
  }
  .stage {
    flex: 999 1 520px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 420px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    padding: 8px 12px;
    font-size: 12px;
    color: var(--text-2);
  }
  .key {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .key i,
  .lns i {
    display: inline-block;
    width: 18px;
    border-top: 2px solid;
    flex: none;
  }
  .dash {
    border-top-style: dashed !important;
  }
  .all {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .spacer {
    flex: 1;
  }
  .ramp {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-3);
  }
  .ramp b {
    width: 84px;
    height: 8px;
    border-radius: 4px;
    background: linear-gradient(90deg, var(--heat-0), var(--heat-1), var(--heat-2), var(--heat-3), var(--heat-4), var(--heat-5));
  }
  .canvas {
    position: relative;
    flex: 1;
    min-height: 380px;
    margin: 0 12px 12px;
    border-radius: var(--radius-lg);
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    overflow: hidden;
  }
  .region {
    position: absolute;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    pointer-events: none;
  }
  .rlab {
    position: absolute;
    left: 8px;
    top: 4px;
    right: 8px;
    display: flex;
    gap: 6px;
    align-items: baseline;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
  }
  .rlab .mono {
    font-size: 11px;
    font-weight: 400;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rerr {
    position: absolute;
    left: 8px;
    top: 24px;
    right: 8px;
    font-size: 12px;
    color: var(--st-failed);
  }
  .dir {
    position: absolute;
    border: 1px solid color-mix(in srgb, var(--border-strong) 70%, transparent);
    border-radius: 3px;
    pointer-events: none;
  }
  .dir i {
    position: absolute;
    left: 4px;
    top: 1px;
    max-width: calc(100% - 8px);
    overflow: hidden;
    white-space: nowrap;
    font: 500 10px var(--mono);
    font-style: normal;
    color: var(--text-3);
  }
  .cell {
    position: absolute;
    padding: 0;
    border: 1px solid var(--bg-subtle);
    border-radius: 2px;
    background: var(--surface);
    transition: opacity var(--ease);
    overflow: hidden;
  }
  .cell.block {
    background: var(--surface-active);
    border-color: var(--bg-subtle);
  }
  .cell em {
    position: absolute;
    left: 4px;
    top: 2px;
    right: 4px;
    font: 500 10px/1.25 var(--mono);
    font-style: normal;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: left;
    pointer-events: none;
  }
  .cell.block em {
    color: var(--text-3);
  }
  .cell.hot {
    cursor: pointer;
  }
  .cell.hot:focus-visible {
    outline: 2px solid var(--focus);
    z-index: 4;
  }
  .canvas.focus .cell:not(.sel):not(.near) {
    opacity: 0.32;
  }
  .cell.sel {
    outline: 2px solid var(--text);
    outline-offset: -1px;
    z-index: 3;
  }
  .canvas.focus .cell.near {
    outline: 1.5px solid var(--text-2);
    outline-offset: -1px;
    z-index: 2;
  }
  svg.links {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 5;
    overflow: visible;
  }
  svg.links path {
    fill: none;
    stroke-linecap: round;
  }
  svg.links path.faint {
    stroke-width: 1;
    opacity: 0.3;
  }
  svg.links circle {
    stroke: var(--bg);
    stroke-width: 1.5;
  }
  .side {
    flex: 1 1 320px;
    min-width: 0;
    max-width: 100%;
    border-left: 1px solid var(--border);
    background: var(--bg-subtle);
    display: flex;
    flex-direction: column;
  }
  .sec {
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .eyebrow {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .rank {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 300px;
    overflow: auto;
  }
  .rank button {
    width: 100%;
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) 72px 30px;
    gap: 8px;
    align-items: center;
    padding: 5px 6px;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    text-align: left;
  }
  .rank button:hover {
    background: var(--surface-hover);
  }
  .rank button.on {
    background: var(--accent-soft);
  }
  .n {
    font: 11px var(--mono);
    color: var(--text-3);
    text-align: right;
  }
  .p {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .p b {
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .p span {
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bar {
    height: 8px;
    border-radius: 2px;
    background: var(--surface-active);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
  }
  .v {
    font-size: 11px;
    text-align: right;
  }
  .ftitle {
    font-size: 14px;
    font-weight: 600;
    word-break: break-all;
  }
  .fsub {
    font-size: 11.5px;
    word-break: break-all;
  }
  .sigs {
    display: flex;
    flex-direction: column;
    gap: 5px;
    margin: 4px 0;
  }
  .sig {
    display: grid;
    grid-template-columns: 112px minmax(0, 1fr) 30px;
    gap: 8px;
    align-items: center;
    font-size: 12px;
    color: var(--text-2);
  }
  .sig.on {
    color: var(--text);
    font-weight: 600;
  }
  .sig .bar {
    height: 9px;
  }
  .why {
    margin: 0;
    padding-left: 16px;
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.55;
  }
  .lns {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .lns button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--text);
    text-align: left;
    font-size: 12px;
    min-width: 0;
  }
  .lns .mono {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .lns small {
    margin-left: auto;
    color: var(--text-3);
    white-space: nowrap;
  }
  .openbtn {
    align-self: flex-start;
    margin-top: 6px;
  }
  .note {
    padding: 0 12px 10px;
    margin: 0;
    font-size: 12px;
  }
  .map.phone .side {
    order: -1;
    border-left: 0;
    border-bottom: 1px solid var(--border);
  }
  .map.phone .stage {
    min-height: 360px;
  }
  .map.phone .canvas {
    min-height: 340px;
  }
</style>
