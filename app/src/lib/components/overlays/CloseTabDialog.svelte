<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { answerCloseTab } from '../../keyboard';

  let { onclose, tabKey }: { onclose: () => void; tabKey: string } = $props();

  const name = $derived(app.items.find((i) => i.key === tabKey)?.title || tabKey);

  function autofocus(node: HTMLButtonElement) {
    node.focus();
  }
</script>

<Modal title={t('tab.endTitle')} {onclose} width={420}>
  <p class="body">{t('tab.endBody', { name })}</p>
  {#snippet footer()}
    <button class="btn ghost" onclick={onclose}>{t('common.cancel')}</button>
    <button class="btn danger" onclick={() => answerCloseTab(true)}>{t('tab.end')}</button>
    <button class="btn primary" use:autofocus onclick={() => answerCloseTab(false)}>{t('tab.hide')}</button>
  {/snippet}
</Modal>

<style>
  .body {
    margin: 0;
    font-size: 12.5px;
    overflow-wrap: anywhere;
  }
</style>
