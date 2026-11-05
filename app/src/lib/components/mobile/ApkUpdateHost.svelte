<script lang="ts">
  import { apk } from '../../mobile/apk-update.svelte';
  const apkSheet = () => import('./ApkUpdateSheet.svelte');
  const tokenSheet = () => import('./GithubTokenSheet.svelte');
</script>

{#if apk.enabled && (apk.sheet || apk.tokenSheet)}
  <div class="layer">
    {#if apk.sheet}{#await apkSheet() then { default: ApkUpdateSheet }}<ApkUpdateSheet />{/await}{/if}
    {#if apk.tokenSheet}{#await tokenSheet() then { default: GithubTokenSheet }}<GithubTokenSheet />{/await}{/if}
  </div>
{/if}

<style>
  .layer {
    position: relative;
    z-index: 70;
  }
</style>
