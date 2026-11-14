<script lang="ts">
  import { agentName } from '../../agents';
  import { app } from '../../state.svelte';
  import AgentGlyph from '../ui/AgentGlyph.svelte';

  let { kind, size = 14 }: { kind: string; size?: number } = $props();
</script>

<span class="icon" title={agentName(app.agents, kind)}>
  <AgentGlyph agent={kind} {size} />
</span>

<style>
  .icon {
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--text-2);
    flex: none;
  }
</style>
