<script lang="ts">
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Monitor from '@lucide/svelte/icons/monitor';
  import Server from '@lucide/svelte/icons/server';
  import { connLabel, HOST_CHIP_DOT } from '../../conn-label';
  import { t } from '../../i18n.svelte';
  import { preload } from '../../idle';
  import { isMockClient } from '../../client';
  import { app } from '../../state.svelte';

  let open = $state(false);
  const menu = preload(() => import('./HostMenu.svelte'));

  const conn = $derived(app.conn);
  const many = $derived(app.hosts.length > 1);
  const label = $derived(many ? t('hosts.count', { n: app.hosts.length }) : conn.target.kind === 'local' ? t('host.local') : conn.target.label);
  const phaseText = $derived(connLabel(conn));
  const tip = $derived([phaseText, conn.error, isMockClient(app.client) ? t('conn.mock') : ''].filter(Boolean).join(' · '));

</script>

<svelte:window onclick={() => (open = false)} />

<div class="wrap" data-tour="host">
  <button
    class="chip"
    data-tour="hosts"
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
    <span class="dot" class:pulse={HOST_CHIP_DOT[conn.phase] === 'var(--st-working)'} style:background={HOST_CHIP_DOT[conn.phase]}></span>
    <ChevronDown size={12} />
  </button>
  {#if open}
    {#await menu() then { default: HostMenu }}<HostMenu {tip} onclose={() => (open = false)} />{/await}
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
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: none;
  }
  .pulse {
    animation: pulse 1.2s ease-in-out 6;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
</style>
