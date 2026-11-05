<script lang="ts">
  import { apk } from '../../mobile/apk-update.svelte';
  const apkSheet = () => import('./ApkUpdateSheet.svelte');
</script>

{#if apk.enabled && apk.sheet}
  <div class="layer">
    {#if apk.sheet}{#await apkSheet() then { default: ApkUpdateSheet }}<ApkUpdateSheet />{/await}{/if}
  </div>
{/if}

<style>
  .layer {
    position: relative;
    z-index: 70;
  }
</style>
