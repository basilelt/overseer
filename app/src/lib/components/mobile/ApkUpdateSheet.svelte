<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import ReleaseNotes from '../ui/ReleaseNotes.svelte';
  import { app } from '../../state.svelte';
  import { apk, at, errorText, formatSize, sheetAction } from '../../mobile/apk-update.svelte';

  const action = $derived(sheetAction(apk));
  const info = $derived(apk.info);
  const date = $derived(info?.date ? new Date(info.date).toLocaleDateString() : '');
  const size = $derived(info ? formatSize(info.size) : '');
  const pct = $derived(apk.progress === null ? null : Math.round(apk.progress * 100));

  function open(url: string) {
    apk.backend?.openUrl(url).catch((e) => app.notify(String(e?.message ?? e), 'error'));
  }

  function close() {
    apk.sheet = false;
  }
</script>

<Modal title={at('title')} onclose={close} width={480}>
  {#if apk.phase === 'checking'}
    <p class="muted">{at('checking')}</p>
  {:else if apk.phase === 'current'}
    <p>{at('current', { version: apk.current })}</p>
  {:else if info}
    <p class="strong">{at('available', { version: info.version })}</p>
    <p class="muted">{at('installed', { version: info.current })}{date ? ` · ${date}` : ''}{size ? ` · ${size}` : ''}</p>
    <ReleaseNotes notes={info.notes} empty={at('noNotes')} onlink={open} />
    {#if apk.phase === 'downloading' || apk.phase === 'verifying'}
      <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={apk.phase === 'downloading' && pct !== null ? pct : undefined}>
        <div class="track"><div class="fill" class:indeterminate={pct === null || apk.phase === 'verifying'} style:width="{apk.phase === 'downloading' && pct !== null ? pct : 100}%"></div></div>
        <span>{apk.phase === 'verifying' ? at('verifying') : `${at('downloading')}${pct === null ? '…' : ` ${pct}%`}`}</span>
      </div>
    {:else if apk.phase === 'permission'}
      <div class="note">
        <p class="strong">{at('permissionTitle')}</p>
        <p class="muted">{at('permissionHint')}</p>
      </div>
    {:else if apk.phase === 'installing'}
      <p class="muted">{at('installingHint')}</p>
    {/if}
  {/if}
  {#if apk.phase === 'error' && apk.error}
    <p class="error" role="alert">{errorText(apk.error)}</p>
  {/if}

  {#snippet footer()}
    {#if !apk.busy}<button class="btn ghost" onclick={close}>{action ? at('later') : at('close')}</button>{/if}
    {#if action === 'install'}
      <button class="btn primary" onclick={() => apk.install()}>{apk.phase === 'error' ? at('retry') : info?.ready ? at('installReady') : at('install')}</button>
    {:else if action === 'allow'}
      <button class="btn primary" onclick={() => apk.allowInstall()}>{at('allow')}</button>
    {:else if action === 'reinstall'}
      <button class="btn primary" onclick={() => apk.install()}>{at('reinstall')}</button>
    {:else if action === 'retry'}
      <button class="btn primary" onclick={() => apk.check(true)}>{at('retry')}</button>
    {:else if apk.busy}
      <button class="btn primary" disabled>{apk.phase === 'checking' ? at('checking') : apk.phase === 'verifying' ? at('verifying') : at('downloading')}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 6px;
    line-height: 1.5;
  }
  .strong {
    font-weight: 600;
  }
  .muted {
    color: var(--text-2);
  }
  .error {
    margin-top: 10px;
    color: var(--st-failed);
    word-break: break-word;
  }
  .note {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  .note p:last-child {
    margin-bottom: 0;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .track {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms linear;
  }
  .fill.indeterminate {
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
</style>
