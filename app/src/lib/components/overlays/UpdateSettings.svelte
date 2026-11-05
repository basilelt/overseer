<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../../state.svelte';
  import { REPO, tokenSourceLabel, updater, ut } from '../../updater.svelte';

  let { row = () => true, hit = () => false }: { row?: (id: string) => boolean; hit?: (id: string) => boolean } = $props();

  let draft = $state('');
  const source = $derived(updater.status?.tokenSource ?? null);
  const line = $derived.by(() => {
    switch (updater.phase) {
      case 'checking':
        return ut('checking');
      case 'current':
        return ut('current', { version: updater.current });
      case 'error':
        return updater.error;
      case 'updated':
        return updater.info ? ut('updated', { version: updater.installedVersion }) : '';
      default:
        return updater.info ? ut('available', { version: updater.info.version }) : '';
    }
  });

  onMount(() => void updater.refreshStatus());

  async function save(token: string) {
    try {
      await updater.setToken(token);
      draft = '';
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  function view() {
    app.open(null);
    updater.dialog = true;
  }
</script>

{#if updater.enabled && !app.mobile}
  <section>
    <h3>{ut('section')}</h3>
    {#if row('update')}
    <div class="row" class:hit={hit('update')}>
      <span class="status">
        <span>{ut('version')} <span class="mono">{updater.current || '—'}</span></span>
        {#if line}<small class:error={updater.phase === 'error'}>{line}</small>{/if}
      </span>
      <div class="actions">
        {#if updater.info && !updater.busy}<button class="btn" onclick={view}>{ut('view')}</button>{/if}
        <button class="btn" disabled={updater.busy} onclick={() => updater.check()}>{ut('check')}</button>
      </div>
    </div>
    {/if}
    {#if row('updateToken')}
    <div class="row stacked" class:hit={hit('updateToken')}>
      <span>{ut('token')}<small>{ut('tokenHint', { repo: REPO })}</small></span>
      <form class="token" onsubmit={(e) => (e.preventDefault(), save(draft))}>
        <input class="mono" type="password" bind:value={draft} placeholder="github_pat_…" autocomplete="off" spellcheck="false" aria-label={ut('token')} />
        <button class="btn" type="submit" disabled={!draft.trim()}>{ut('tokenSave')}</button>
        {#if source === 'settings'}<button class="btn ghost" type="button" onclick={() => save('')}>{ut('tokenClear')}</button>{/if}
      </form>
      <small>{tokenSourceLabel(source)}</small>
    </div>
    {/if}
  </section>
{/if}

<style>
  section {
    padding: 10px 0 12px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 36px;
  }
  .row.stacked {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    margin-top: 8px;
  }
  .row > span,
  .status {
    display: flex;
    flex-direction: column;
  }
  small {
    color: var(--text-3);
    font-size: 11px;
  }
  small.error {
    color: var(--st-failed);
  }
  .actions,
  .token {
    display: flex;
    gap: 6px;
  }
  .token input {
    flex: 1;
    min-width: 0;
  }
  .row.hit {
    margin: 0 -8px;
    padding-left: 8px;
    padding-right: 8px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
</style>
