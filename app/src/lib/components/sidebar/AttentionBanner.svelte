<script lang="ts">
  import type { AttentionEntry } from '../../attention';
  import { t } from '../../i18n.svelte';
  import { itemMenuAt } from '../../workspace/wsactions';

  let { entries, onselect, pathOf }: { entries: AttentionEntry[]; onselect: (entry: AttentionEntry) => void; pathOf?: (entry: AttentionEntry) => string | undefined } = $props();
</script>

{#if entries.length}
  <div class="banner" role="region" aria-label={t('session.state.attention')}>
    <div class="head">{t('session.state.attention')}</div>
    {#each entries as entry (entry.key)}
      {@const path = pathOf?.(entry)}
      <button class="item" type="button" onclick={() => onselect(entry)} oncontextmenu={(e) => itemMenuAt(entry.key, entry.title, e)}>
        <span class="dot" aria-hidden="true"></span>
        <span class="title">{entry.title}</span>
        {#if entry.detail}<span class="detail">{entry.detail}</span>{/if}
        {#if path}<span class="path">{path}</span>{/if}
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
    flex-wrap: wrap;
    align-items: center;
    column-gap: 8px;
    min-height: 26px;
    padding: 0 6px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    min-width: 0;
  }
  .item:hover {
    background: var(--attention-hover);
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--state-attention);
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
  .path {
    flex-basis: 100%;
    padding: 0 0 4px 16px;
    font-size: 11px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(:root.phone) .banner {
    padding: 8px;
    border-radius: 12px;
  }
  :global(:root.phone) .head {
    padding: 0 4px 2px;
    font-size: 12px;
  }
  :global(:root.phone) .item {
    min-height: var(--touch);
    padding: 4px 8px;
    border-radius: 10px;
  }
  :global(:root.phone) .path {
    padding-bottom: 0;
    font-size: 12px;
  }
</style>
