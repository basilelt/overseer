<script lang="ts">
  import { agentName } from '../../agents';
  import { app } from '../../state.svelte';
  import { GROUP_COLOR } from '../sidebar/AgentIcon.svelte';
  import AgentGlyph from '../ui/AgentGlyph.svelte';

  const MAX = 8;
  const active = $derived(app.groups.filter((g) => g.id === 'needs_input' || g.id === 'working').flatMap((g) => g.items));
</script>

{#if active.length}
  <div class="avatars">
    {#each active.slice(0, MAX) as item (item.key)}
      <button
        class="avatar"
        class:selected={app.selectedKey === item.key}
        title={`${item.title} · ${agentName(app.agents, item.kind)}`}
        aria-label={item.title}
        onclick={() => app.select(item.key)}
      >
        <AgentGlyph agent={item.kind} size={13} />
        <span class="badge" style:background={GROUP_COLOR[item.group]}></span>
      </button>
    {/each}
    {#if active.length > MAX}<span class="more faint">+{active.length - MAX}</span>{/if}
  </div>
{/if}

<style>
  .avatars {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    padding: 2px;
    overflow: hidden;
  }
  .avatar {
    position: relative;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--surface-2);
    border: 1px solid var(--border);
    display: grid;
    place-items: center;
    color: var(--text-2);
    transition: border-color var(--ease);
  }
  .avatar:hover,
  .avatar.selected {
    border-color: var(--accent);
    color: var(--text);
  }
  .badge {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 2px solid var(--bg);
  }
  .more {
    font-size: 11px;
    padding-left: 2px;
  }
</style>
