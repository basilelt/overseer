<script lang="ts">
  import { t } from '../../i18n.svelte';
  import { reviewCounts } from '../../review/counts.svelte';
  import { app } from '../../state.svelte';

  let { tabKey }: { tabKey: string } = $props();

  $effect(() => {
    if (app.connected && app.client) reviewCounts.ensure();
  });

  const n = $derived(reviewCounts.forKey(tabKey));
</script>

{#if n > 0}
  <span class="badge" title={t('review.unresolved', { n })} aria-label={t('review.unresolved', { n })}>{n}</span>
{/if}

<style>
  .badge {
    display: inline-grid;
    place-items: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 10px;
    font-weight: 600;
    flex: none;
  }
</style>
