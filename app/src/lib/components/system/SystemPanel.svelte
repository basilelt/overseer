<script lang="ts">
  import SquareArrowOutUpRight from '@lucide/svelte/icons/square-arrow-out-up-right';
  import X from '@lucide/svelte/icons/x';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { ui } from '../../mobile/ui.svelte';
  import { st } from '../../system/strings';
  import { update, openInFocused } from '../../workspace.svelte';
  import { newTabKey } from '../../tabs';
  import SystemView from './SystemView.svelte';

  let { onclose }: { onclose: () => void } = $props();
  const phone = $derived(app.mobile || ui.layout === 'phone');

  function inTab() {
    const key = newTabKey('system', 'host');
    onclose();
    app.view = 'agents';
    update((l) => openInFocused(l, key));
    app.selectedKey = key;
  }
</script>

{#if phone}
  <div class="sheet" role="dialog" aria-modal="true" aria-label={st('title')}>
    <header>
      <h2>{st('title')}</h2>
      <button class="icon-btn" aria-label={t('common.close')} onclick={onclose}><X size={18} /></button>
    </header>
    <div class="body"><SystemView compact /></div>
  </div>
{:else}
  <Modal title={st('title')} {onclose} width={860}>
    <SystemView />
    {#snippet footer()}
      <button class="btn ghost" onclick={inTab}><SquareArrowOutUpRight size={13} /> {st('openTab')}</button>
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
