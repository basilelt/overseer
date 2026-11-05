<script lang="ts">
  import type { TabInfo } from '../tabs';
  import GitGraphTab from './GitGraphTab.svelte';

  let { tab }: { tabKey: string; tab: TabInfo; focused: boolean } = $props();
</script>

<div class="host">
  {#key tab.arg}<GitGraphTab projectPath={tab.arg} />{/key}
</div>

<style>
  .host {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
</style>
