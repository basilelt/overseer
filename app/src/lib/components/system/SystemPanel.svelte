<script lang="ts">
  import { st } from '../../system/strings';
  import PanelShell from './PanelShell.svelte';
  import SystemView from './SystemView.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<PanelShell kind="system" title={st('title')} openTab={st('openTab')} {onclose}>
  {#snippet children(compact)}
    <SystemView {compact} />
  {/snippet}
</PanelShell>
