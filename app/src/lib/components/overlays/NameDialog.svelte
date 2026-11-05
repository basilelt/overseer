<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const d = app.nameDialog;
  let value = $state(d?.value ?? '');
  let error = $state('');
  let busy = $state(false);
  const trimmed = $derived(value.trim());

  async function submit() {
    if (!d || !trimmed || busy) return;
    busy = true;
    error = '';
    try {
      await d.submit(trimmed);
      onclose();
    } catch (e) {
      error = String((e as Error)?.message ?? e);
    } finally {
      busy = false;
    }
  }

  function autofocus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<Modal title={d?.title ?? ''} {onclose} width={380}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
  >
    <label>
      <span>{d?.label}</span>
      <input use:autofocus bind:value maxlength="80" aria-invalid={!!error} oninput={() => (error = '')} />
      {#if error}<em class="err" role="alert">{error}</em>{/if}
    </label>
  </form>
  {#snippet footer()}
    <button class="btn ghost" onclick={onclose}>{t('common.cancel')}</button>
    <button class="btn primary" disabled={!trimmed || busy} onclick={submit}>{t('common.save')}</button>
  {/snippet}
</Modal>

<style>
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label > span {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .err {
    font-style: normal;
    font-size: 12px;
    color: var(--st-failed);
  }
</style>
