<script lang="ts">
  import { ust } from '../../usage/strings';
  import PanelShell from '../system/PanelShell.svelte';
  import UsageView from './UsageView.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<PanelShell kind="usage" title={ust('title')} openTab={ust('openTab')} {onclose}>
  {#snippet children(compact)}
    <UsageView {compact} />
  {/snippet}
</PanelShell>
