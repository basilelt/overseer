<script lang="ts">
  import * as CMD from '../../tauri-commands';
  import { t, type Key } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { diagnostics, terminalCommand, unstableIdentity, type LocalNetworkView } from '../../localnet';
  import type { LocalNetworkReport } from '../../types';

  let {
    report,
    view,
    error,
    onretry,
    onback,
  }: { report: LocalNetworkReport; view: LocalNetworkView; error?: string; onretry: () => void; onback: () => void } = $props();

  let retrying = $state(false);
  const host = $derived(report.endpoint ?? report.host ?? '');
  const rows = $derived(diagnostics(report));
  const fromTerminal = $derived(unstableIdentity(report) ? terminalCommand(report) : null);

  async function openSettings() {
    try {
      await app.client?.openLocalNetworkSettings();
    } catch {}
  }

  async function relaunch() {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke(CMD.updateRelaunch, { reason: 'relaunch for local network access' });
    } catch {}
  }

  async function retry() {
    retrying = true;
    app.localNet = await app.probeLocalNetwork(report.host ?? undefined);
    retrying = false;
    onretry();
  }
</script>

<section class="ln" role="alert">
  <h3>{view === 'permission' ? t('localnet.title') : t('localnet.titleUnreachable', { host })}</h3>
  <p>{view === 'permission' ? t('localnet.body') : t('localnet.bodyUnreachable', { host })}</p>
  {#if unstableIdentity(report)}<p class="note">{t('localnet.adhoc')}</p>{/if}
  {#if view === 'permission'}<p class="note">{t('localnet.notListed')}</p>{/if}
  <details>
    <summary>{t('localnet.details')}</summary>
    <dl>
      {#each rows as [id, value] (id)}
        <dt>{t(`localnet.d.${id}` as Key)}</dt>
        <dd class="mono">{value}</dd>
      {/each}
    </dl>
    {#if fromTerminal}<p class="note">{t('localnet.terminal')} <code class="mono">{fromTerminal}</code></p>{/if}
    {#if error}<pre>{error}</pre>{/if}
  </details>
</section>
<div class="actions">
  <button class="btn" onclick={onback}>{t('common.back')}</button>
  <span class="grow"></span>
  <button class="btn" onclick={openSettings}>{t('localnet.open')}</button>
  <button class="btn" onclick={relaunch}>{t('localnet.relaunch')}</button>
  <button class="btn primary" disabled={retrying} onclick={retry}>{retrying ? t('localnet.checking') : t('localnet.retry')}</button>
</div>

<style>
  .ln {
    margin-top: 14px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  h3 {
    margin: 0 0 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  p {
    margin: 0 0 8px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .note {
    font-size: 12px;
    color: var(--text-3);
  }
  details {
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-3);
  }
  summary {
    cursor: pointer;
    user-select: none;
    -webkit-user-select: none;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 12px;
    margin: 8px 0 0;
  }
  dt {
    color: var(--text-3);
  }
  .note code {
    user-select: text;
    -webkit-user-select: text;
    overflow-wrap: anywhere;
  }
  dd {
    margin: 0;
    color: var(--text-2);
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }
  pre {
    margin: 8px 0 0;
    font-family: var(--mono);
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-2);
    user-select: text;
    -webkit-user-select: text;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .grow {
    flex: 1;
  }
</style>
