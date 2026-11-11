<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { preload } from '../../idle';

  const list = preload(() => import('./NewSessionList.svelte'));

  let open = $state(false);
  let root: HTMLDivElement;

  function close(refocus: boolean) {
    open = false;
    if (refocus) root.querySelector<HTMLElement>('.icon-btn')?.focus();
  }
</script>

<svelte:window onpointerdown={(e) => open && !root.contains(e.target as Node) && (open = false)} />

<div class="wrap" bind:this={root}>
  <button
    class="icon-btn"
    class:active={open}
    title={t('title.new')}
    aria-label={t('title.new')}
    aria-haspopup="menu"
    aria-expanded={open}
    disabled={!app.connected}
    onclick={() => (open = !open)}><Plus size={15} /></button
  >
  {#if open}
    {#await list() then { default: NewSessionList }}<NewSessionList onclose={close} />{/await}
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
</style>
