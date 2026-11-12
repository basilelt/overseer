<script lang="ts">
  import PanelShell from './PanelShell.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<PanelShell kind="system" title="Panel title" openTab="Open in tab" {onclose}>
  {#snippet children(compact)}
    content:{compact ? 'compact' : 'full'}
  {/snippet}
</PanelShell>
