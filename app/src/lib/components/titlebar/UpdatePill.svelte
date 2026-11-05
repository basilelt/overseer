<script lang="ts">
  import { pillLabel, updater } from '../../updater.svelte';
  const updateDialog = () => import('../overlays/UpdateDialog.svelte');

  const label = $derived(pillLabel(updater));
</script>

{#if label}
  <button class="pill" class:busy={updater.busy} onclick={() => (updater.dialog = true)}>
    <span class="dot"></span>{label}
  </button>
{/if}
{#if updater.dialog}
  {#await updateDialog() then { default: UpdateDialog }}<UpdateDialog onclose={() => (updater.dialog = false)} />{/await}
{/if}

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 11px;
    background: var(--surface);
    color: var(--text);
    font-size: 11.5px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    transition: border-color var(--ease);
  }
  .pill:hover {
    border-color: var(--border-strong);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .busy .dot {
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
</style>
