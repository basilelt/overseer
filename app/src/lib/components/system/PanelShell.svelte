<script lang="ts">
  import type { Snippet } from 'svelte';
  import SquareArrowOutUpRight from '@lucide/svelte/icons/square-arrow-out-up-right';
  import X from '@lucide/svelte/icons/x';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { update, openInFocused } from '../../workspace/workspace.svelte';
  import { newTabKey } from '../../tabs/tabs';
  import { phoneOrMobile } from '../../mobile/phone.svelte';

  let {
    kind,
    title,
    openTab,
    onclose,
    children,
  }: {
    kind: 'system' | 'usage';
    title: string;
    openTab: string;
    onclose: () => void;
    children: Snippet<[boolean]>;
  } = $props();
  const phone = $derived(phoneOrMobile());

  function inTab() {
    const key = newTabKey(kind, 'host');
    onclose();
    app.view = 'agents';
    update((l) => openInFocused(l, key));
    app.selectedKey = key;
  }
</script>

{#if phone}
  <div class="sheet" role="dialog" aria-modal="true" aria-label={title}>
    <header>
      <h2>{title}</h2>
      <button class="icon-btn" aria-label={t('common.close')} onclick={onclose}><X size={18} /></button>
    </header>
    <div class="body">{@render children(true)}</div>
  </div>
{:else}
  <Modal {title} {onclose} width={860}>
    {@render children(false)}
    {#snippet footer()}
      <button class="btn ghost" onclick={inTab}><SquareArrowOutUpRight size={13} /> {openTab}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .sheet {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    background: var(--bg);
    display: flex;
    flex-direction: column;
    padding-top: env(safe-area-inset-top);
    padding-bottom: env(safe-area-inset-bottom);
  }
  .sheet header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px 6px 16px;
  }
  .sheet h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .body {
    flex: 1;
    overflow-y: auto;
    padding: 4px 16px 24px;
  }
</style>
