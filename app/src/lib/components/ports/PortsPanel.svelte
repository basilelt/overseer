<script lang="ts">
  import Globe from '@lucide/svelte/icons/globe';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Copy from '@lucide/svelte/icons/copy';
  import X from '@lucide/svelte/icons/x';
  import CircleStop from '@lucide/svelte/icons/circle-stop';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import Modal from '../ui/Modal.svelte';
  import { t } from '../../i18n.svelte';
  import { app, projectName } from '../../state.svelte';
  import { saveSettings, settings } from '../../settings/settings.svelte';
  import { openExternal, ports } from '../../ports/ports.svelte';
  import { stopProcess } from '../../ports/ports-kill';
  import { parseIgnore, parsePort, type PortRow } from '../../ports/ports';

  let { onclose }: { onclose: () => void } = $props();

  let remoteInput = $state('');
  let localInput = $state('');
  let error = $state('');
  let busy = $state(false);
  let ignoreText = $state(settings.portsIgnore);

  const remote = $derived(ports.state.remote);
  const rows = $derived(ports.rows);
  const invalidIgnore = $derived(parseIgnore(ignoreText).invalid);
  const msg = (e: unknown) => String((e as Error)?.message ?? e);

  async function forward(remotePort?: number, localPort: number | null = null) {
    error = '';
    if (remotePort === undefined) {
      const r = parsePort(remoteInput);
      const l = parsePort(localInput);
      if (!r) return void (error = t('ports.invalidPort', { value: remoteInput.trim() || '—' }));
      if (l === undefined) return void (error = t('ports.invalidPort', { value: localInput.trim() }));
      remotePort = r;
      localPort = l;
    }
    busy = true;
    try {
      await app.requireClient().forwardPort(remotePort, localPort);
      remoteInput = localInput = '';
    } catch (e) {
      error = msg(e);
    } finally {
      busy = false;
    }
  }

  async function stop(row: PortRow) {
    try {
      await app.requireClient().unforwardPort(row.port);
    } catch (e) {
      app.notify(msg(e), 'error');
    }
  }

  function browse(url: string, termId?: string) {
    onclose();
    app.openUrl(url, { termId });
  }

  async function copy(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      app.notify(t('ports.copied', { url }));
    } catch (e) {
      app.notify(msg(e), 'error');
    }
  }

  function saveIgnore() {
    if (ignoreText !== settings.portsIgnore) saveSettings({ portsIgnore: ignoreText.trim() });
  }

  const project = (path?: string) => (path ? projectName(app.allProjects, path) : '');
  const focus = (el: HTMLInputElement) => {
    if (app.overlayArg === 'forward') el.focus();
  };
  const onEnter = (e: KeyboardEvent) => e.key === 'Enter' && forward();
</script>

<Modal title={t('ports.title')} {onclose} width={780}>
  {#if remote}
    <div class="forward">
      <label>
        <span>{t('ports.remotePort')}</span>
        <input class="mono" use:focus bind:value={remoteInput} onkeydown={onEnter} inputmode="numeric" placeholder="5173" spellcheck="false" />
      </label>
      <ArrowRight size={14} />
      <label>
        <span>{t('ports.localPort')}</span>
        <input class="mono" bind:value={localInput} onkeydown={onEnter} inputmode="numeric" placeholder={t('ports.localAuto')} spellcheck="false" />
      </label>
      <button class="btn primary" disabled={busy || !remoteInput.trim()} onclick={() => forward()}>{t('ports.forward')}</button>
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  {:else if app.connected}
    <p class="hint">{t('ports.localHint')}</p>
  {/if}

  {#if rows.length}
    <table>
      <thead>
        <tr>
          <th>{t(remote ? 'ports.col.remote' : 'ports.col.port')}</th>
          <th>{t('ports.col.local')}</th>
          <th>{t('ports.col.process')}</th>
          <th>{t('ports.col.project')}</th>
          {#if remote}<th>{t('ports.col.source')}</th>{/if}
          <th class="actions"><span class="sr-only">{t('ports.col.actions')}</span></th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row (row.port)}
          <tr>
            <td class="mono port" title={row.addr}>{row.port}</td>
            <td class="mono">
              {#if row.error}<span class="failed clip" title={row.error}>{row.error}</span>{:else if row.local}{row.local}{:else}<span class="faint">{t('ports.notForwarded')}</span>{/if}
            </td>
            <td>
              {#if row.process}{row.process}{#if row.pid}<span class="faint mono pid">{row.pid}</span>{/if}{:else}<span class="faint">—</span>{/if}
            </td>
            <td title={row.project}><span class="clip">{project(row.project) || '—'}</span></td>
            {#if remote}<td class="faint">{row.source ? t(row.source === 'auto' ? 'ports.source.auto' : 'ports.source.manual') : '—'}</td>{/if}
            <td class="actions">
              {#if row.url}
                {@const url = row.url}
                <button class="icon-btn" title={t('ports.openBrowser')} aria-label={t('ports.openBrowser')} onclick={() => browse(url, row.termId)}><Globe size={14} /></button>
                <button class="icon-btn" title={t('ports.openExternal')} aria-label={t('ports.openExternal')} onclick={() => openExternal(url).catch((e) => app.notify(msg(e), 'error'))}><ExternalLink size={14} /></button>
                <button class="icon-btn" title={t('ports.copyUrl')} aria-label={t('ports.copyUrl')} onclick={() => copy(url)}><Copy size={14} /></button>
              {/if}
              {#if row.detected && row.pid}
                <button class="icon-btn" title={t('term.kill')} aria-label={t('term.kill')} onclick={() => stopProcess(app, row)}><CircleStop size={14} /></button>
              {/if}
              {#if remote && (row.forwarded || row.error)}
                <button class="icon-btn" title={t('ports.stop')} aria-label={t('ports.stop')} onclick={() => stop(row)}><X size={14} /></button>
              {:else if remote}
                <button class="btn" disabled={busy} onclick={() => forward(row.port)}>{t('ports.forward')}</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="empty">{t(app.connected ? 'ports.empty' : 'ports.disconnected')}</p>
  {/if}

  <section class="settings">
    <h3>{t('ports.settings')}</h3>
    <label class="row">
      <span>{t('ports.autoForward')}<small>{t('ports.autoForwardHint')}</small></span>
      <input type="checkbox" checked={settings.portsAutoForward} onchange={(e) => saveSettings({ portsAutoForward: e.currentTarget.checked })} />
    </label>
    <label class="row">
      <span>{t('ports.ignore')}<small>{invalidIgnore.length ? t('ports.ignoreInvalid', { list: invalidIgnore.join(', ') }) : t('ports.ignoreHint')}</small></span>
      <input class="mono ignore" class:bad={invalidIgnore.length} bind:value={ignoreText} onchange={saveIgnore} onblur={saveIgnore} placeholder="22, 5432 9000-9010" spellcheck="false" />
    </label>
    <label class="row">
      <span>{t('ports.localhostOnly')}<small>{t('ports.localhostOnlyHint')}</small></span>
      <input type="checkbox" checked={settings.portsLocalhostOnly} onchange={(e) => saveSettings({ portsLocalhostOnly: e.currentTarget.checked })} />
    </label>
  </section>
</Modal>

<style>
  .forward {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    padding-bottom: 12px;
    color: var(--text-3);
  }
  .forward :global(svg) {
    margin-bottom: 8px;
  }
  .forward label {
    display: grid;
    gap: 4px;
    font-size: 11px;
  }
  .forward input {
    width: 140px;
    height: 28px;
    color: var(--text);
  }
  .forward .btn {
    margin-left: auto;
  }
  .error,
  .failed {
    color: var(--st-failed);
  }
  .error {
    margin: -4px 0 12px;
    font-size: 12px;
    white-space: pre-wrap;
    user-select: text;
    -webkit-user-select: text;
  }
  .hint,
  .empty {
    margin: 0 0 12px;
    color: var(--text-2);
  }
  .empty {
    padding: 18px 0;
    text-align: center;
    color: var(--text-3);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th {
    text-align: left;
    font-weight: 500;
    font-size: 11px;
    color: var(--text-3);
    padding: 0 8px 6px;
    border-bottom: 1px solid var(--border);
  }
  td {
    padding: 4px 8px;
    height: 34px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
    user-select: text;
    -webkit-user-select: text;
  }
  .port {
    font-weight: 600;
  }
  .pid {
    margin-left: 6px;
  }
  .clip {
    display: inline-block;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    vertical-align: middle;
  }
  .actions {
    text-align: right;
    width: 1%;
  }
  .actions .btn {
    height: 24px;
    font-size: 12px;
  }
  .settings {
    margin-top: 16px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 6px 0;
  }
  .row span {
    display: grid;
  }
  small {
    font-size: 11px;
    color: var(--text-3);
  }
  input[type='checkbox'] {
    accent-color: var(--accent);
  }
  .ignore {
    width: 220px;
  }
  .ignore.bad {
    border-color: var(--st-failed);
  }
</style>
