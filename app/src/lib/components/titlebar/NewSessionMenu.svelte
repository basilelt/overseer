<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { preload } from '../../idle';
  import { dismiss } from '../../dismiss';

  const list = preload(() => import('./NewSessionList.svelte'));

  let open = $state(false);
  let root: HTMLDivElement;

  function close(refocus: boolean) {
    open = false;
    if (refocus) root.querySelector<HTMLElement>('.icon-btn')?.focus();
  }
</script>

<div class="wrap" data-tour="new-session" bind:this={root} use:dismiss={{ active: open, onclose: () => (open = false) }}>
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
