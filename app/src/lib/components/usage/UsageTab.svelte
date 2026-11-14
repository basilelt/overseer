<script lang="ts">
  import type { TabInfo } from '../../tabs/tabs';
  import UsageView from './UsageView.svelte';

  let {}: { tabKey: string; tab: TabInfo; focused: boolean } = $props();
</script>

<div class="host">
  <div class="inner"><UsageView /></div>
</div>

<style>
  .host {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    background: var(--bg);
  }
  .inner {
    max-width: 1100px;
    margin: 0 auto;
    padding: 16px 20px 32px;
  }
</style>
