<script lang="ts">
  import type { AttentionEntry } from '../../attention';
  import { t } from '../../i18n.svelte';

  let { entries, onselect }: { entries: AttentionEntry[]; onselect: (entry: AttentionEntry) => void } = $props();
</script>

{#if entries.length}
  <div class="banner" role="region" aria-label={t('session.state.attention')}>
    <div class="head">{t('session.state.attention')}</div>
    {#each entries as entry (entry.key)}
      <button class="item" type="button" data-key={entry.key} onclick={() => onselect(entry)}>
        <span class="dot" style:--c="var(--state-attention)" aria-hidden="true"></span>
        <span class="title">{entry.title}</span>
        {#if entry.detail}<span class="detail">{entry.detail}</span>{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .banner {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin: 6px 6px 0;
    padding: 6px;
    border-radius: 8px;
    background: var(--surface-2);
  }
  .head {
    font-size: 11px;
    font-weight: 600;
    color: var(--state-attention);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    padding: 0 6px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    min-width: 0;
  }
  .item:hover {
    background: var(--surface-hover);
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
  }
  .title {
    flex: none;
    max-width: 50%;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
