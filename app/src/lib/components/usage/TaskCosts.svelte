<script lang="ts">
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { agentName } from '../../agents';
  import { findProfile } from '../../profiles';
  import { costRows, type CostBy } from '../../cost';
  import { formatCost } from '../../usage/format';
  import AgentGlyph from '../ui/AgentGlyph.svelte';

  let by = $state<CostBy>('profile');
  const bys: { id: CostBy; label: () => string }[] = [
    { id: 'profile', label: () => t('cost.byProfile') },
    { id: 'ticket', label: () => t('cost.byTicket') },
    { id: 'task', label: () => t('cost.byTask') },
  ];
  const rows = $derived(costRows(app.data.tasks, by, (id) => findProfile(app.profiles, id)?.name ?? id, t('cost.noProfile')));
</script>

<section class="block" data-tour="task-costs">
  <div class="h3row">
    <h3 title={t('cost.perTaskHint')}>{t('cost.perTask')}</h3>
    <div class="seg" role="tablist">
      {#each bys as b (b.id)}
        <button role="tab" aria-selected={by === b.id} class:on={by === b.id} onclick={() => (by = b.id)}>{b.label()}</button>
      {/each}
    </div>
  </div>
  {#if !rows.length}
    <p class="faint small">{t('cost.empty')}</p>
  {:else}
    <table>
      <tbody>
        {#each rows as r (r.key)}
          <tr>
            <td class="name">
              {#if r.agent}<AgentGlyph agent={r.agent} size={11} />{/if}
              <span title={r.agent ? `${r.label} · ${agentName(app.agents, r.agent)}` : r.label}>{r.label}</span>
              {#if by !== 'task'}<span class="faint small"> · {r.tasks === 1 ? t('cost.tasksOne', { done: r.finished }) : t('cost.tasks', { n: r.tasks, done: r.finished })}</span>{/if}
            </td>
            <td class="num avg mono faint" title={by === 'profile' && r.perTask != null ? t('cost.avg', { cost: formatCost(r.perTask) }) : undefined}>{by === 'profile' && r.perTask != null ? t('cost.avgShort', { cost: formatCost(r.perTask) }) : ''}</td>
            <td class="num mono">{formatCost(r.cost, r.partial)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<style>
  .h3row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 6px;
  }
  h3 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .seg button {
    padding: 2px 8px;
    font-size: 11px;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--surface-active);
    color: var(--text);
  }
  .small {
    font-size: 12px;
    margin: 0;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }
  td {
    padding: 4px 0;
    border-top: 1px solid var(--border);
  }
  td.name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  td.name :global(svg) {
    vertical-align: -1px;
    margin-right: 4px;
  }
  .num {
    text-align: right;
    white-space: nowrap;
    width: 72px;
  }
  .avg {
    width: 96px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
