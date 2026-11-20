<script lang="ts">
  import type { Level } from '../../severity';

  let { used, pace = null, tone = 'ok' }: { used: number; pace?: number | null; tone?: Level } = $props();
</script>

<span class="bar">
  <span class="fill {tone}" style:width="{Math.min(100, Math.max(0, used))}%"></span>
  {#if pace != null}<span class="pace" style:left="{pace}%"></span>{/if}
</span>

<style>
  .bar {
    --use-warn: #b45309;
    position: relative;
    display: block;
    height: 6px;
    background: var(--surface-2);
    border-radius: 3px;
    overflow: hidden;
  }
  :global(:root[data-theme='dark']) .bar {
    --use-warn: #f59e0b;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--text-3);
  }
  .fill.warn {
    background: var(--use-warn);
  }
  .fill.crit {
    background: var(--st-failed);
  }
  .pace {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--text);
    opacity: 0.7;
  }
</style>
