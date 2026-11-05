<script lang="ts" module>
  import type { GroupId } from '../../grouping';

  export const GROUP_COLOR: Record<GroupId, string> = {
    needs_input: 'var(--st-input)',
    needs_testing: 'var(--st-testing)',
    working: 'var(--st-working)',
    pushed: 'var(--st-done)',
    idle: 'var(--border-strong)',
    held: 'var(--st-held)',
  };
</script>

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
