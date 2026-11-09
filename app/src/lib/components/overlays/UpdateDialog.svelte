<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import ReleaseNotes from '../ui/ReleaseNotes.svelte';
  import { dialogAction, updater, ut } from '../../updater.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const action = $derived(dialogAction(updater));
  const info = $derived(updater.info);
  const date = $derived(info?.date ? new Date(info.date).toLocaleDateString() : '');

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
      <p class="strong">{ut(info.kind === 'msix' ? 'updatedMsix' : 'updated', { version: updater.installedVersion })}</p>
    {:else}
      <p class="strong">{ut('available', { version: info.version })}</p>
      <p class="muted">{ut('installed', { version: info.current })}{date ? ` · ${date}` : ''}</p>
    {/if}
    <ReleaseNotes notes={info.notes} empty={ut('noNotes')} onlink={open} />
    {#if updater.phase === 'updated' && info.kind === 'msix'}
      <p class="muted">{ut('msixDone')}</p>
    {/if}
    {#if updater.phase === 'downloading' || updater.phase === 'installing' || updater.phase === 'restarting'}
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
