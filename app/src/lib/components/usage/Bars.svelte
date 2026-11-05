<script lang="ts">
  import { barHeights } from '../../usage/format';

  let { values, labels, titles, height = 64, label }: { values: number[]; labels: string[]; titles: string[]; height?: number; label: string } = $props();
  const h = $derived(barHeights(values));
  const every = $derived(values.length > 14 ? 5 : 1);
</script>

<div class="bars" role="img" aria-label={label}>
  <div class="cols" style:height="{height}px">
    {#each h as v, i (i)}
      <span class="col" class:last={i === h.length - 1} title={titles[i]}><span class="fill" style:height="{v}%"></span></span>
    {/each}
  </div>
  <div class="axis faint">
    {#each labels as l, i (i)}
      <span class="tick">{(h.length - 1 - i) % every === 0 ? l : ''}</span>
    {/each}
  </div>
</div>

<style>
  .bars {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .cols {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    border-bottom: 1px solid var(--border);
  }
  .col {
    flex: 1;
    height: 100%;
    display: flex;
    align-items: flex-end;
    min-width: 2px;
  }
  .col:hover .fill {
    opacity: 0.9;
  }
  .fill {
    display: block;
    width: 100%;
    background: var(--text-3);
    opacity: 0.55;
    border-radius: 2px 2px 0 0;
  }
  .col.last .fill {
    background: var(--accent, var(--text-2));
    opacity: 0.8;
  }
  .axis {
    display: flex;
    gap: 3px;
    font-size: 10px;
  }
  .tick {
    flex: 1;
    min-width: 2px;
    text-align: center;
    white-space: nowrap;
    overflow: visible;
  }
</style>
