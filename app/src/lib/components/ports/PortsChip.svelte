<script lang="ts">
  import EthernetPort from '@lucide/svelte/icons/ethernet-port';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { ports } from '../../ports.svelte';

  const label = $derived(t(ports.chip.forwarded ? 'ports.chipForwarded' : 'ports.chipOpen', { n: ports.chip.n }));
</script>

{#if ports.chip.n > 0}
  {#key ports.flash}
    <button class="chip" class:flash={ports.flash > 0} data-tour="chips" title={label} aria-label={label} onclick={() => app.open('ports')}>
      <EthernetPort size={13} />
      <span class="mono">{ports.chip.n}</span>
    </button>
  {/key}
{/if}

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 13px;
    color: var(--text-2);
    font-size: 12px;
    transition: background var(--ease);
  }
  .chip:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .flash {
    animation: flash 1.6s ease-out 1;
  }
  @keyframes flash {
    from {
      border-color: var(--accent);
      color: var(--accent);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .flash {
      animation: none;
    }
  }
</style>
