<script lang="ts">
  import { t } from '../../i18n.svelte';
  import { agentName, taskAgents } from '../../agents';
  import { canOrchestrate, toggleReviewer, toggleWorker } from '../../quorum';
  import type { AgentInfo, Quorum } from '../../types';

  let { agents, agent, on, value, onon, onchange }: { agents: AgentInfo[]; agent: string; on: boolean; value: Quorum; onon: (on: boolean) => void; onchange: (q: Quorum) => void } = $props();

  const allowed = $derived(canOrchestrate(agent));
  const list = $derived(taskAgents(agents).filter((a) => a.installed));
</script>

<div class="field" data-tour="quorum">
  <label class="tog">
    <input type="checkbox" class="switch" checked={on && allowed} disabled={!allowed} onchange={(e) => onon(e.currentTarget.checked)} />
    <span>{t('quorum.label')}</span>
  </label>
  {#if !allowed}
    <p class="hint faint">{t('quorum.needsOrchestrator')}</p>
  {:else if !on}
    <p class="hint faint">{t('quorum.toggle')}</p>
  {:else}
    <div class="group" role="group" aria-label={t('quorum.workers')}>
      <span class="gl">{t('quorum.workers')}</span>
      {#each list as a (a.id)}
        <button type="button" class="chip" aria-pressed={value.workers.includes(a.id)} onclick={() => onchange(toggleWorker(value, a.id))}>{agentName(agents, a.id)}</button>
      {/each}
    </div>
    <div class="group" role="group" aria-label={t('quorum.reviewers')}>
      <span class="gl">{t('quorum.reviewers')}</span>
      {#each list as a (a.id)}
        <button type="button" class="chip" aria-pressed={value.reviewers.includes(a.id)} onclick={() => onchange(toggleReviewer(value, a.id))}>{agentName(agents, a.id)}</button>
      {/each}
    </div>
    <p class="hint faint">{t('quorum.hint')}</p>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .tog {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .group {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .gl {
    flex: none;
    min-width: 96px;
    font-size: 12px;
    color: var(--text-2);
  }
  :global(.phone) .gl {
    flex-basis: 100%;
  }
  .chip {
    padding: 1px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 11px;
    line-height: 18px;
    color: var(--text-2);
  }
  .chip[aria-pressed='true'] {
    color: var(--accent);
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .hint {
    margin: 0;
    font-size: 12px;
  }
</style>
