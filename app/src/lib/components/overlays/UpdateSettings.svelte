<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../../state.svelte';
  import { updater, ut } from '../../updater.svelte';

  let { row = () => true, hit = () => false }: { row?: (id: string) => boolean; hit?: (id: string) => boolean } = $props();

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
    .actions {
    display: flex;
    gap: 6px;
  }
  .row.hit {
    margin: 0 -8px;
    padding-left: 8px;
    padding-right: 8px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
</style>
