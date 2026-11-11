<script lang="ts">
  import Monitor from '@lucide/svelte/icons/monitor';
  import Server from '@lucide/svelte/icons/server';
  import { t } from '../../i18n.svelte';
  import { settings } from '../../settings.svelte';
  import { app } from '../../state.svelte';
  import type { HostTarget } from '../../types';

  let { tip, onclose }: { tip: string; onclose: () => void } = $props();
  const conn = $derived(app.conn);

  function pick(h: HostTarget) {
    onclose();
    app.connect(h);
  }

  async function disconnect() {
    onclose();
    await app.disconnect();
  }
</script>

<div class="menu" role="menu">
  <div class="status faint">{tip}</div>
  {#if !app.mobile}
    <button role="menuitem" onclick={() => pick({ kind: 'local', label: 'Local' })}>
      <Monitor size={13} />
      {t('connect.local')}
    </button>
  {/if}
  {#each settings.recentHosts as h (h.host)}
    <button role="menuitem" onclick={() => pick(h)}><Server size={13} />{h.label}</button>
  {/each}
  <hr />
  <button role="menuitem" onclick={() => app.open('connect')}>{t('host.connect')}</button>
  {#if conn.phase === 'connected' && conn.target.kind !== 'local'}
    <button role="menuitem" onclick={disconnect}>{t('host.disconnect')}</button>
  {/if}
</div>

<style>
  .menu {
    position: absolute;
    top: 32px;
    right: 0;
    min-width: 220px;
    padding: 4px;
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    z-index: 40;
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border-radius: 4px;
    text-align: left;
  }
  .menu button:hover {
    background: var(--surface-hover);
  }
  .status {
    padding: 4px 8px 6px;
    font-size: 12px;
  }
  hr {
    border: 0;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
</style>
