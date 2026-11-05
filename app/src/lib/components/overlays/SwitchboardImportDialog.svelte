<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import { t } from '../../i18n.svelte';
  import { reloadApp, resetImport, runImport, switchboard, switchboardNever } from '../../switchboard.svelte';
  import { countKeys } from '../../switchboard';

  let { onclose, from = null }: { onclose: () => void; from?: string | null } = $props();

  resetImport();

  const status = $derived(switchboard.status);
  const done = $derived(switchboard.done);

  $effect(() => {
    if (!done) return;
    const id = setTimeout(reloadApp, 2000);
    return () => clearTimeout(id);
  });

  function never() {
    switchboardNever();
    onclose();
  }
</script>

<Modal title={t('switchboard.title')} {onclose} width={480}>
  {#if done}
    {@const c = countKeys(done, t)}
    <p class="ok" role="status">{t('switchboard.done', c)}</p>
    {#if done.stopped}<p class="muted">{t('switchboard.stopped')}</p>{/if}
    <p class="muted">{t('switchboard.reloading')}</p>
  {:else if status}
    {@const c = countKeys(status, t)}
    <p class="muted intro">{t('switchboard.intro')}</p>
    <ul class="counts">
      <li>{c.sessions}</li>
      <li>{c.tasks}</li>
      <li>{c.workspaces}</li>
    </ul>
    {#if status.dir}<p class="faint small">{t('switchboard.dir', { dir: status.dir })}</p>{/if}
    {#if status.running}<p class="muted">{t('switchboard.running')}</p>{/if}
    {#if status.hooks}<p class="muted">{t('switchboard.hooks')}</p>{/if}
    <p class="muted">{t('switchboard.settingsNote')}</p>
  {/if}
  {#if switchboard.error}<p class="err" role="alert">{t('switchboard.failed', { error: switchboard.error })}</p>{/if}
  {#snippet footer()}
    {#if done}
      <button class="btn primary" onclick={reloadApp}>{t('switchboard.reload')}</button>
    {:else}
      {#if from !== 'settings'}<button class="btn ghost" onclick={never}>{t('switchboard.never')}</button>{/if}
      <button class="btn" onclick={onclose}>{from === 'settings' ? t('common.cancel') : t('switchboard.notNow')}</button>
      <button class="btn primary" disabled={switchboard.busy || !status} onclick={runImport}>{switchboard.busy ? t('switchboard.importing') : t('switchboard.import')}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 10px;
    font-size: 12.5px;
  }
  .counts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
  }
  .counts li {
    padding: 3px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 12px;
  }
  .small {
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .ok {
    color: var(--st-input);
  }
  .err {
    color: var(--st-failed);
    overflow-wrap: anywhere;
  }
</style>
