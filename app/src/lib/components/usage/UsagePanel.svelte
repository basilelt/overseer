<script lang="ts">
  import SquareArrowOutUpRight from '@lucide/svelte/icons/square-arrow-out-up-right';
  import X from '@lucide/svelte/icons/x';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { ui } from '../../mobile/ui.svelte';
  import { ust } from '../../usage/strings';
  import { update, openInFocused } from '../../workspace.svelte';
  import { newTabKey } from '../../tabs';
  import UsageView from './UsageView.svelte';

  let { onclose }: { onclose: () => void } = $props();
  const phone = $derived(app.mobile || ui.layout === 'phone');

  function inTab() {
    const key = newTabKey('usage', 'host');
    onclose();
    app.view = 'agents';
    update((l) => openInFocused(l, key));
    app.selectedKey = key;
  }
</script>

{#if phone}
  <div class="sheet" role="dialog" aria-modal="true" aria-label={ust('title')}>
    <header>
      <h2>{ust('title')}</h2>
      <button class="icon-btn" aria-label={t('common.close')} onclick={onclose}><X size={18} /></button>
    </header>
    <div class="body"><UsageView compact /></div>
  </div>
{:else}
  <Modal title={ust('title')} {onclose} width={860}>
    <UsageView />
    {#snippet footer()}
      <button class="btn ghost" onclick={inTab}><SquareArrowOutUpRight size={13} /> {ust('openTab')}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .sheet {
    position: fixed;
    inset: 0;
    z-index: 50;
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
