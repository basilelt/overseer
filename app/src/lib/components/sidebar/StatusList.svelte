<script lang="ts">
  import { onMount } from 'svelte';
  import type { GroupId } from '../../grouping';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import GroupHeader from './GroupHeader.svelte';
  import SessionCard from './SessionCard.svelte';
  import { flatten, moveSelection, rowHeight, windowRows } from './virtual';
  import { itemMenu } from '../../wsactions';
  import { onPresence, presence } from '../../presence';

  let { filter }: { filter: string } = $props();

  let collapsed = $state(new Set<GroupId>());
  let cursor = $state<string | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(600);
  let now = $state(Date.now());
  let list: HTMLDivElement;

  const rows = $derived(flatten(app.groups, collapsed, filter));
  const win = $derived(windowRows(rows, scrollTop, viewport));

  onMount(() => {
    let id: ReturnType<typeof setInterval> | undefined;
    const sync = () => {
      clearInterval(id);
      id = undefined;
      if (!presence().visible) return;
      now = Date.now();
      id = setInterval(() => (now = Date.now()), 30_000);
    };
    sync();
    const off = onPresence(sync);
    return () => {
      off();
      clearInterval(id);
    };
  });

  function toggle(id: GroupId, open?: boolean) {
    const next = new Set(collapsed);
    if (open ?? next.has(id)) next.delete(id);
    else next.add(id);
    collapsed = next;
  }

  function reveal(key: string) {
    const i = rows.findIndex((r) => r.type === 'card' && r.item.key === key);
    if (i < 0) return;
    const top = win.offsets[i];
    const bottom = top + rowHeight(rows[i]);
    if (top < list.scrollTop) list.scrollTop = top - 30;
    else if (bottom > list.scrollTop + viewport) list.scrollTop = bottom - viewport;
  }

  function select(e: MouseEvent | undefined, key: string) {
    if (e && (e.metaKey || e.ctrlKey || e.shiftKey)) {
      const next = new Set(app.multi);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      if (app.selectedKey && !next.size) next.add(app.selectedKey);
      app.multi = next;
      return;
    }
    app.multi = new Set();
    app.select(key);
  }

  function onKey(e: KeyboardEvent) {
    const current = cursor ?? app.selectedKey;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      cursor = moveSelection(rows, current, e.key === 'ArrowDown' ? 1 : -1);
      if (cursor) reveal(cursor);
    } else if (e.key === 'Enter' && current) {
      e.preventDefault();
      app.select(current);
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      const item = app.items.find((i) => i.key === current);
      if (item) toggle(item.group, e.key === 'ArrowRight');
    }
  }
</script>

<div
    class="list"
    role="listbox"
    tabindex="0"
    aria-label={t('sidebar.title')}
    bind:this={list}
    bind:clientHeight={viewport}
    onscroll={() => (scrollTop = list.scrollTop)}
    onkeydown={onKey}
    onblur={() => (cursor = null)}
  >
    {#if !rows.length}
      <p class="empty faint">{t('sidebar.empty')}</p>
    {/if}
    <div class="spacer" style:height="{win.total}px">
      {#each rows.slice(win.start, win.end) as row, i (row.type === 'header' ? `h:${row.id}` : row.item.key)}
        <div class="row" style:transform="translateY({win.offsets[win.start + i]}px)" style:height="{rowHeight(row)}px">
          {#if row.type === 'header'}
            <GroupHeader id={row.id} count={row.count} collapsed={row.collapsed} ontoggle={() => toggle(row.id)} />
          {:else}
            {@const key = row.item.key}
            <div class="cell" class:cursor={cursor === key} class:multi={app.multi.has(key)} role="presentation" oncontextmenu={(e) => (e.preventDefault(), app.openMenu(e, row.item.title, itemMenu(key)))}>
              <SessionCard item={row.item} {now} selected={app.selectedKey === key} onselect={(e) => select(e, key)} />
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>

<style>
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 0 6px 8px;
    outline: none;
  }
  .spacer {
    position: relative;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
  }
  .cell {
    border-radius: var(--radius);
  }
  .cell.multi {
    box-shadow: inset 0 0 0 1px var(--accent);
    background: var(--accent-soft);
  }
  .list:focus-visible .cell.cursor {
    box-shadow: inset 0 0 0 1px var(--focus);
  }
  .empty {
    padding: 24px 12px;
    text-align: center;
  }
</style>
