<script lang="ts">
  import Zap from '@lucide/svelte/icons/zap';
  import Bot from '@lucide/svelte/icons/bot';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { settings } from '../../settings.svelte';
  import { findAgent, pickAgent, isAgentId } from '../../agents';
  import { organizer } from '../../organize.svelte';

  const agentId = $derived(pickAgent(app.agents, settings.defaultAgent));
  const agent = $derived(findAgent(app.agents, agentId));
  const canAgent = $derived(app.connected && !!agent?.installed && isAgentId(agent.id));

  function withAgent() {
    if (!agent) return;
    const same = agentId === settings.defaultAgent;
    void organizer.withAgent({
      agent: agentId,
      model: same ? settings.defaultModel || undefined : undefined,
      effort: same && agent.efforts?.includes(settings.defaultEffort) ? settings.defaultEffort : undefined,
    });
  }
</script>

<Modal title={t('org.title')} onclose={() => organizer.close()} width={520}>
  {#if organizer.phase === 'waiting'}
    <div class="wait">
      <p class="strong">{t('org.waiting', { agent: agent?.name ?? agentId })}</p>
      <p class="small hint">{t('org.waitingHint')}</p>
      {#if organizer.manual}<p class="note small">{t('org.manual')}</p>{/if}
      {#if organizer.late}<p class="note small">{t('org.late')}</p>{/if}
    </div>
  {:else}
    <p class="faint small intro">{t('org.intro')}</p>
    <div class="choices">
      <button class="choice" disabled={organizer.busy || !app.connected} onclick={() => void organizer.quick()}>
        <Zap size={18} />
        <span><span class="strong">{t('org.quick')}</span><small>{t('org.quickHint')}</small></span>
      </button>
      <button class="choice" disabled={organizer.busy || !canAgent} onclick={withAgent}>
        <Bot size={18} />
        <span
          ><span class="strong">{canAgent && agent ? t('org.agent', { agent: agent.name }) : t('org.noAgent')}</span><small>{t('org.agentHint')}</small></span
        >
      </button>
    </div>
  {/if}
  {#snippet footer()}
    <span class="spacer"></span>
    {#if organizer.phase === 'waiting' && (organizer.late || organizer.manual)}
      <button class="btn" disabled={organizer.busy} onclick={() => void organizer.quick()}>{t('org.useQuick')}</button>
    {/if}
    <button class="btn" onclick={() => organizer.close()}>{t('common.close')}</button>
  {/snippet}
</Modal>

<style>
  .intro,
  .small {
    margin: 0 0 12px;
    font-size: 12px;
  }
  .strong {
    font-weight: 600;
    display: block;
  }
  .choices {
    display: grid;
    gap: 8px;
  }
  .choice {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    text-align: left;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
  }
  .choice:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .choice:disabled {
    opacity: 0.55;
  }
  .choice small {
    display: block;
    margin-top: 2px;
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.4;
  }
  .wait {
    display: grid;
    gap: 6px;
  }
  .wait p {
    margin: 0;
  }
  .hint {
    color: var(--text-2);
  }
  .note {
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--bg-subtle);
    color: var(--text-2);
  }
</style>
