<script lang="ts">
  import type { Snippet } from 'svelte';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';

  let { title, meta, id, order, open = $bindable(true), children }: { title: string; meta?: string; id?: string; order?: number; open?: boolean; children: Snippet } = $props();
</script>

<section data-sec={id} style:order>
  <button class="head" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="chev" class:open><ChevronRight size={12} /></span>
    <span class="title">{title}</span>
    {#if meta}<span class="meta faint">{meta}</span>{/if}
  </button>
  {#if open}<div class="body">{@render children()}</div>{/if}
</section>

<style>
  section {
    border-top: 1px solid var(--border);
    padding: 4px 0 8px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 30px;
    padding: 0 14px 0 10px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .head:hover {
    color: var(--text);
  }
  .chev {
    display: inline-grid;
    transition: transform var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .title {
    flex: 1;
    text-align: left;
  }
  .meta {
    text-transform: none;
    letter-spacing: 0;
    font-weight: 500;
  }
  .body {
    padding: 0 14px;
  }
</style>
