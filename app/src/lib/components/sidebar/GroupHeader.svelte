<script lang="ts">
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import type { GroupId } from '../../grouping';
  import { t } from '../../i18n.svelte';
  import { GROUP_COLOR } from './AgentIcon.svelte';

  let { id, count, collapsed, ontoggle }: { id: GroupId; count: number; collapsed: boolean; ontoggle: () => void } = $props();
</script>

<button class="header" aria-expanded={!collapsed} tabindex="-1" onclick={ontoggle}>
  <span class="chev" class:open={!collapsed}><ChevronRight size={12} /></span>
  <span class="dot" style:background={GROUP_COLOR[id]}></span>
  <span class="label">{t(`group.${id}`)}</span>
  <span class="count">{count}</span>
</button>

<style>
  .header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 30px;
    padding: 0 8px 0 4px;
    color: var(--text-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .header:hover {
    color: var(--text);
  }
  .chev {
    display: inline-grid;
    transition: transform var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .label {
    flex: 1;
    text-align: left;
  }
  .count {
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
    font-weight: 500;
  }
</style>
