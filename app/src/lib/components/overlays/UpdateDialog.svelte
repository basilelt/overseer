<script lang="ts">
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import ReleaseNotes from '../ui/ReleaseNotes.svelte';
  import { BREW_UPGRADE, brewStepLabel, dialogAction, updater, ut } from '../../updater.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const action = $derived(dialogAction(updater));
  const info = $derived(updater.info);
  const date = $derived(info?.date ? new Date(info.date).toLocaleDateString() : '');
  const brew = $derived(info?.kind === 'homebrew');
  const untrusted = $derived(updater.phase === 'error' && updater.brewError?.kind === 'untrusted' ? updater.brewError.tap : null);
  let copied = $state(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(BREW_UPGRADE);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {}
  }

  function open(url: string) {
    updater.backend?.openUrl(url).catch((e) => app.notify(String(e?.message ?? e), 'error'));
  }
</script>

<Modal title={ut('title')} {onclose} width={480}>
  {#if updater.phase === 'checking'}
    <p class="muted">{ut('checking')}</p>
  {:else if updater.phase === 'current'}
    <p>{ut('current', { version: updater.current })}</p>
  {:else if info}
    {#if updater.phase === 'updated'}
      <p class="strong">{ut('updated', { version: updater.installedVersion })}</p>
    {:else}
      <p class="strong">{ut('available', { version: info.version })}</p>
      <p class="muted">{ut('installed', { version: info.current })}{date ? ` · ${date}` : ''}</p>
    {/if}
    <ReleaseNotes notes={info.notes} empty={ut('noNotes')} onlink={open} />
    {#if brew}
      {#if updater.phase === 'updating' || (updater.phase === 'restarting' && updater.log.length)}
        <div class="status">
          <LoaderCircle size={14} class="spin" />
          <span>{updater.phase === 'restarting' ? ut('pill.restarting') : brewStepLabel(updater.step)}</span>
        </div>
        {#if updater.lastLine}<div class="last" title={updater.lastLine}>{updater.lastLine}</div>{/if}
      {:else if updater.phase !== 'updated' && updater.phase !== 'restarting'}
        <p class="muted">{ut('brew')} {ut('brewOr')}</p>
        <div class="cmd">
          <code>{BREW_UPGRADE}</code>
          <button class="btn ghost" onclick={copy}>{copied ? ut('copied') : ut('copy')}</button>
        </div>
      {/if}
      {#if updater.log.length}
        <details class="log">
          <summary>{ut('log')}</summary>
          <pre>{updater.log.join('\n')}</pre>
        </details>
      {/if}
    {:else if action === 'release'}
      <p class="muted">{ut('package')}</p>
    {/if}
    {#if updater.phase === 'downloading' || updater.phase === 'installing' || (updater.phase === 'restarting' && !brew)}
      <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={updater.progress === null ? undefined : Math.round(updater.progress * 100)}>
        <div class="track"><div class="fill" class:indeterminate={updater.progress === null || updater.phase !== 'downloading'} style:width="{updater.phase === 'downloading' && updater.progress !== null ? updater.progress * 100 : 100}%"></div></div>
        <span>{ut(updater.phase === 'downloading' ? 'downloading' : updater.phase === 'installing' ? 'installing' : 'restarting')}{updater.phase === 'downloading' && updater.progress !== null ? ` ${Math.round(updater.progress * 100)}%` : '…'}</span>
      </div>
    {/if}
  {/if}
  {#if (updater.phase === 'error' || updater.phase === 'updated') && updater.error}
    <p class="error">{updater.error}</p>
  {/if}

  {#snippet footer()}
    {#if action === 'restart'}
      <button class="btn ghost" onclick={onclose}>{ut('later')}</button>
      <button class="btn primary" onclick={() => updater.restart()}>{ut('restart')}</button>
    {:else if action === 'install'}
      <button class="btn ghost" onclick={onclose}>{ut('later')}</button>
      <button class="btn primary" onclick={() => updater.install()}>{updater.phase === 'error' ? ut('retry') : ut('install')}</button>
    {:else if action === 'brew'}
      {#if updater.phase === 'updating' || updater.phase === 'restarting'}
        <button class="btn primary" disabled>{updater.phase === 'restarting' ? ut('pill.restarting') : ut('pill.updating')}</button>
      {:else}
        <button class="btn ghost" onclick={onclose}>{ut('later')}</button>
        {#if untrusted}
          <button class="btn primary" onclick={() => updater.brewUpgrade(untrusted)}>{ut('trust')}</button>
        {:else}
          <button class="btn primary" onclick={() => updater.brewUpgrade()}>{updater.phase === 'error' ? ut('retry') : ut('update')}</button>
        {/if}
      {/if}
    {:else if action === 'release' && info}
      <button class="btn primary" onclick={() => open(info.releaseUrl)}>{ut('openRelease')}</button>
    {:else if action === 'retry'}
      <button class="btn" onclick={() => updater.check(true)}>{ut('retry')}</button>
    {:else if !updater.busy}
      <button class="btn" onclick={onclose}>{ut('later')}</button>
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
  .cmd {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 4px 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  .cmd code {
    flex: 1;
    font-family: var(--mono);
    font-size: 12px;
    user-select: all;
    -webkit-user-select: all;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }
  .last {
    margin: 4px 0 0 22px;
    color: var(--text-2);
    font-family: var(--mono);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .log {
    margin-top: 10px;
    font-size: 12px;
    color: var(--text-2);
  }
  .log summary {
    cursor: pointer;
    user-select: none;
    -webkit-user-select: none;
  }
  .log pre {
    margin: 6px 0 0;
    padding: 8px 10px;
    max-height: 180px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
    font-family: var(--mono);
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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
