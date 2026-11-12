<script lang="ts">
  import type { SidebarItem } from '../../grouping';
  import { t } from '../../i18n.svelte';
  import { age } from '../../time';
  import AgentIcon, { GROUP_COLOR, STATE_COLOR } from './AgentIcon.svelte';
  import { isShellItem } from '../../agents';

  let { item, selected, now, onselect }: { item: SidebarItem; selected: boolean; now: number; onselect: (e?: MouseEvent) => void } = $props();
</script>

<button
  class="card"
  class:selected
  style:--bar={GROUP_COLOR[item.group]}
  data-key={item.key}
  role="option"
  aria-selected={selected}
  tabindex="-1"
  onclick={(e) => onselect(e)}
>
  <div class="top">
    <AgentIcon kind={item.kind} />
    <span class="title">{item.title}</span>
    <span class="st" title={t(`session.state.${item.state}`)} style:--c={STATE_COLOR[item.state]}><span class="sd"></span>{t(`session.state.${item.state}`)}</span>
    <span class="age faint">{age(item.at, now)}</span>
  </div>
  {#if item.goal && isShellItem(item)}<div class="line muted mono path">{item.goal}</div>
  {:else if item.goal}<div class="line muted"><span class="faint">{t('card.goal')}</span> {item.goal}</div>{/if}
  {#if item.status}<div class="line faint">{item.status}</div>{/if}
</button>

<style>
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    height: 72px;
    padding: 7px 10px 7px 12px;
    border-radius: var(--radius);
    text-align: left;
    overflow: hidden;
    transition: background var(--ease);
  }
  .card::before {
    content: '';
    position: absolute;
    left: 0;
    top: 9px;
    bottom: 9px;
    width: 2px;
    border-radius: 2px;
    background: var(--bar);
  }
  .card:hover {
    background: var(--surface-hover);
  }
  .card.selected {
    background: var(--accent-soft);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .title {
    flex: 1;
    min-width: 0;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .st {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: 0 1 auto;
    min-width: 0;
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    color: var(--text-2);
  }
  .sd {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c);
  }
  .age {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .line {
    font-size: 12px;
    line-height: 16px;
    padding-left: 30px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path {
    font-size: 11px;
  }
</style>
