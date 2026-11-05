<script lang="ts">
  import type { Snippet } from 'svelte';
  import ChevronsLeft from '@lucide/svelte/icons/chevrons-left';
  import ChevronsRight from '@lucide/svelte/icons/chevrons-right';
  import { t, type Key } from '../../i18n.svelte';
  import type { TaskStatus } from '../../types';

  let {
    status,
    count,
    collapsed = false,
    collapsible = false,
    target = false,
    empty = false,
    el = $bindable(),
    list = $bindable(),
    ontoggle,
    extra,
    children,
  }: {
    status: TaskStatus;
    count: number;
    collapsed?: boolean;
    collapsible?: boolean;
    target?: boolean;
    empty?: boolean;
    el?: HTMLElement;
    list?: HTMLElement;
    ontoggle?: () => void;
    extra?: Snippet;
    children: Snippet;
  } = $props();

  const label = $derived(t(`kanban.col.${status}` as Key));
</script>

<section class="col {status}" class:collapsed class:target bind:this={el} aria-label={label}>
  {#if collapsed}
    <button class="rail" onclick={ontoggle} title={t('kanban.expand')} aria-label="{t('kanban.expand')}: {label}" aria-expanded="false">
      <ChevronsRight size={14} />
      <span class="count">{count}</span>
      <span class="vlabel">{label}</span>
    </button>
  {:else}
    <header>
      <span class="bar"></span>
      <h3>{label}</h3>
      <span class="count">{count}</span>
      {@render extra?.()}
      {#if collapsible}
        <button class="icon-btn collapse" onclick={ontoggle} title={t('kanban.collapse')} aria-label={t('kanban.collapse')} aria-expanded="true">
          <ChevronsLeft size={14} />
        </button>
      {/if}
    </header>
    <div class="list" role="list" bind:this={list}>
      {#if target && empty}<div class="drop">{t('kanban.dropHere')}</div>{/if}
      {@render children()}
      {#if count === 0 && !(target && empty)}<div class="empty faint">{t('kanban.empty')}</div>{/if}
    </div>
  {/if}
</section>

<style>
  .col {
    --c: var(--text-3);
    flex: 1 1 0;
    min-width: 260px;
    max-width: 380px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    transition:
      border-color var(--ease),
      background var(--ease);
  }
  .col.auto {
    --c: var(--st-working);
  }
  .col.in_progress {
    --c: var(--st-working);
  }
  .col.in_testing {
    --c: var(--st-testing);
  }
  .col.completed {
    --c: var(--st-done);
  }
  .col.target {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .col.collapsed {
    flex: 0 0 44px;
    min-width: 44px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 10px 8px 12px;
    min-height: 44px;
  }
  .bar {
    width: 3px;
    height: 12px;
    border-radius: 2px;
    background: var(--c);
  }
  h3 {
    margin: 0;
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .count {
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .collapse {
    margin-left: auto;
    width: 24px;
    height: 24px;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 2px 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .drop {
    flex: none;
    height: 40px;
    display: grid;
    place-items: center;
    border: 1px dashed var(--accent);
    border-radius: var(--radius);
    color: var(--accent);
    font-size: 11.5px;
  }
  .empty {
    padding: 18px 0;
    text-align: center;
    font-size: 12px;
  }
  .rail {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 12px 0;
    color: var(--text-2);
    border-radius: var(--radius-lg);
  }
  .rail:hover {
    background: var(--surface-hover);
  }
  .vlabel {
    writing-mode: vertical-rl;
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
</style>
