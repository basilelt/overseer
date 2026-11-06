<script lang="ts" module>
  import type { GroupId } from '../../grouping';
  import type { SessionState } from '../../session-state';

  export const STATE_COLOR: Record<SessionState, string> = {
    running: 'var(--state-running)',
    attention: 'var(--state-attention)',
    open: 'var(--state-open)',
    paused: 'var(--state-paused)',
    closed: 'var(--state-closed)',
  };

  export const GROUP_COLOR: Record<GroupId, string> = {
    needs_input: 'var(--st-input)',
    needs_testing: 'var(--st-testing)',
    running: 'var(--st-working)',
    pushed: 'var(--st-done)',
    ready: 'var(--border-strong)',
    paused: 'var(--st-paused)',
    stopped: 'var(--st-held)',
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
