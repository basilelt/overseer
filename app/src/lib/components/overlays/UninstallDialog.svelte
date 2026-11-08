<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { onMount } from 'svelte';
  import { applyUninstall, canQuit, planUninstall, quitApp, resetUninstall, uninstall } from '../../uninstall.svelte';

  let { onclose = () => {} }: { onclose?: () => void } = $props();

  const host = $derived(app.conn.target.kind === 'local' ? t('uninstall.thisComputer') : app.conn.target.label);
  const report = $derived(uninstall.report);
  const busy = $derived(uninstall.phase === 'planning' || uninstall.phase === 'applying');
  const close = () => {
    if (busy) return;
    resetUninstall();
    onclose();
  };

  onMount(() => {
    resetUninstall();
    void planUninstall();
  });
  const title = $derived(uninstall.phase === 'done' ? t('uninstall.doneTitle') : t('uninstall.title', { host }));
</script>

<Modal {title} onclose={close} width={560}>
  {#if uninstall.phase === 'error'}
    <p class="err" role="alert">{t('uninstall.error', { error: uninstall.error })}</p>
  {:else if busy}
    <p class="note">{uninstall.phase === 'planning' ? t('uninstall.planning') : t('uninstall.applying')}</p>
  {:else if report}
    {#if uninstall.phase === 'confirm'}
      <p class="note">{t('uninstall.intro', { host })}</p>
    {:else}
      <p class="note">{t(canQuit() ? 'uninstall.doneQuit' : 'uninstall.done', { host })}</p>
    {/if}
    <h4>{t(uninstall.phase === 'confirm' ? 'uninstall.willRemove' : 'uninstall.removed', { n: report.removed.length })}</h4>
    {#if report.removed.length}
      <ul>
        {#each report.removed as r (r.kind + r.path)}
          <li><span class="kind">{t(`uninstall.kind.${r.kind}`)}</span><code>{r.path}</code></li>
        {/each}
      </ul>
    {:else}
      <p class="faint">{t('uninstall.nothing')}</p>
    {/if}
    {#if report.kept.length}
      <h4>{t('uninstall.kept', { n: report.kept.length })}</h4>
      <ul>
        {#each report.kept as k (k.kind + k.path)}
          <li><span class="kind">{t(`uninstall.kind.${k.kind}`)}</span><code>{k.path}</code><small>{k.reason}</small></li>
        {/each}
      </ul>
    {/if}
    {#if report.errors.length}
      <h4 class="bad">{t('uninstall.errors', { n: report.errors.length })}</h4>
      <ul>
        {#each report.errors as e (e.kind + e.path)}
          <li><span class="kind">{t(`uninstall.kind.${e.kind}`)}</span><code>{e.path}</code><small class="bad">{e.error}</small></li>
        {/each}
      </ul>
    {/if}
  {/if}
  {#snippet footer()}
    {#if uninstall.phase === 'confirm'}
      <button class="btn ghost" onclick={close}>{t('common.cancel')}</button>
      <button class="btn danger" onclick={applyUninstall}>{t('uninstall.confirm')}</button>
    {:else if uninstall.phase === 'done' && canQuit()}
      <button class="btn ghost" onclick={close}>{t('common.close')}</button>
      <button class="btn primary" onclick={quitApp}>{t('uninstall.quit')}</button>
    {:else}
      <button class="btn ghost" disabled={busy} onclick={close}>{t('common.close')}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .note,
  .err {
    margin: 0 0 8px;
    font-size: 12.5px;
  }
  .err,
  .bad {
    color: var(--st-failed);
  }
  h4 {
    margin: 12px 0 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 220px;
    overflow: auto;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 8px;
    padding: 3px 0;
    font-size: 12px;
  }
  .kind {
    flex: none;
    min-width: 72px;
    color: var(--text-2);
  }
  code {
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: 11.5px;
  }
  small {
    flex-basis: 100%;
    padding-left: 80px;
    color: var(--text-3);
  }
</style>
