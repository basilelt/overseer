<script lang="ts">
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Monitor from '@lucide/svelte/icons/monitor';
  import Server from '@lucide/svelte/icons/server';
  import { connLabel } from '../../conn-label';
  import { t } from '../../i18n.svelte';
  import { settings } from '../../settings.svelte';
  import { app } from '../../state.svelte';
  import type { ConnPhase, HostTarget } from '../../types';

  let open = $state(false);

  const PHASE_COLOR: Record<ConnPhase, string> = {
    idle: 'var(--st-held)',
    connecting: 'var(--st-working)',
    checking: 'var(--st-working)',
    downloading: 'var(--st-working)',
    installing: 'var(--st-working)',
    starting: 'var(--st-working)',
    reconnecting: 'var(--st-working)',
    connected: 'var(--st-input)',
    error: 'var(--st-failed)',
  };

  const conn = $derived(app.conn);
  const label = $derived(conn.target.kind === 'local' ? t('host.local') : conn.target.label);
  const phaseText = $derived(connLabel(conn));
  const tip = $derived([phaseText, conn.error, app.client?.kind === 'mock' ? t('conn.mock') : ''].filter(Boolean).join(' · '));

  function pick(h: HostTarget) {
    open = false;
    app.connect(h);
  }

  async function disconnect() {
    open = false;
    await app.disconnect();
  }
</script>

<svelte:window onclick={() => (open = false)} />

<div class="wrap" data-tour="host">
  <button
    class="chip"
    title={tip}
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={(e) => {
      e.stopPropagation();
      open = !open;
    }}
  >
    {#if conn.target.kind === 'local'}<Monitor size={13} />{:else}<Server size={13} />{/if}
    <span class="label">{label}</span>
    <span class="dot" class:pulse={PHASE_COLOR[conn.phase] === 'var(--st-working)'} style:background={PHASE_COLOR[conn.phase]}></span>
    <ChevronDown size={12} />
  </button>
  {#if open}
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
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 13px;
    color: var(--text-2);
    transition: background var(--ease);
  }
  .chip:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .label {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font-weight: 500;
  }
  .pulse {
    animation: pulse 1.2s ease-in-out 6;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
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
