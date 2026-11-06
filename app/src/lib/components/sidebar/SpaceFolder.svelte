<script lang="ts">
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import FolderClosed from '@lucide/svelte/icons/folder';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import { t } from '../../i18n.svelte';
  import type { SpaceNode } from '../../spaces';
  import StatePills from './StatePills.svelte';

  // Depth 0 is a section title, deeper folders are rows; the pinned section (no folder) cannot fold.
  let { node, ontoggle }: { node: Extract<SpaceNode, { type: 'folder' }>; ontoggle: () => void } = $props();

  const name = $derived(node.folder?.name ?? t('ws.pinned'));
  const meta = $derived(
    [node.folders ? (node.folders === 1 ? t('ws.foldersOne') : t('ws.folders', { n: node.folders })) : '', node.spaces === 1 ? t('ws.spacesOne') : t('ws.spaces', { n: node.spaces })]
      .filter(Boolean)
      .join(' · '),
  );
</script>

{#if node.depth === 0}
  {#if node.folder}
    <button class="section" type="button" aria-expanded={node.open} onclick={ontoggle}>
      <span class="chev" class:open={node.open}><ChevronRight size={11} /></span>
      <span class="name">{name}</span>
      {#if !node.open}<StatePills summary={node.summary} hideZero />{/if}
      <span class="meta">{meta}</span>
    </button>
  {:else}
    <div class="section"><span class="name">{name}</span></div>
  {/if}
{:else}
  <button class="row" type="button" aria-expanded={node.open} onclick={ontoggle}>
    <span class="chev" class:open={node.open}><ChevronRight size={12} /></span>
    {#if node.open}<FolderOpen size={15} />{:else}<FolderClosed size={15} />{/if}
    <span class="name">{name}</span>
    <StatePills summary={node.summary} hideZero />
  </button>
{/if}

<style>
  .section,
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-width: 0;
    border: 0;
    background: none;
    text-align: left;
  }
  .section {
    padding: 8px 6px 2px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .row {
    height: 28px;
    padding: 0 6px;
    border-radius: 6px;
    color: var(--text-2);
  }
  .row:hover {
    background: var(--surface-hover);
  }
  .row .name {
    color: var(--text);
    font-weight: 500;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    flex: none;
    font-weight: 500;
    letter-spacing: 0;
    text-transform: none;
  }
  .chev {
    display: inline-flex;
    flex: none;
    transition: transform 0.12s;
  }
  .chev.open {
    transform: rotate(90deg);
  }
</style>
