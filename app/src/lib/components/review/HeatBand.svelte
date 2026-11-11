<script lang="ts">
  import { t } from '../../i18n.svelte';
  import { heat, type Band } from '../../review/map.svelte';

  let { bands, current = null, names, onpick }: { bands: Band[]; current?: string | null; names: (id: string) => string; onpick: (id: string) => void } = $props();
</script>

<nav class="band" aria-label={t('map.band')}>
  {#each bands as b (b.id)}
    <button class:on={b.id === current} style:flex-grow={b.share} style:background={heat(b.heat)} title={names(b.id)} aria-label={names(b.id)} onclick={() => onpick(b.id)}></button>
  {/each}
</nav>

<style>
  .band {
    flex: none;
    width: 12px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px 3px;
    border-right: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  button {
    flex: 1 1 0;
    min-height: 3px;
    padding: 0;
    border: 0;
    border-radius: 1px;
    cursor: pointer;
  }
  button:hover {
    filter: brightness(1.15);
  }
  button.on {
    outline: 1.5px solid var(--text);
    outline-offset: 1px;
  }
</style>
