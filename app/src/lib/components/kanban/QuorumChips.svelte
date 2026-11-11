<script lang="ts">
  import { agentName } from '../../agents';
  import { t, type Key } from '../../i18n.svelte';
  import { isReviewer, tally } from '../../quorum';
  import { app } from '../../state.svelte';
  import type { Task } from '../../types';

  let { task }: { task: Task } = $props();

  const names = (ids: string[]) => ids.map((id) => agentName(app.agents, id)).join(', ');
  const worker = $derived(isReviewer(task) && task.parentId ? app.data.tasks.find((x) => x.id === task.parentId) : undefined);
  const sum = $derived(task.votes ? tally(task.votes) : null);
</script>

{#if task.quorum}
  <span class="q" title={t('quorum.badgeTitle', { workers: names(task.quorum.workers), reviewers: names(task.quorum.reviewers) })}>{t('quorum.badge')}</span>
{/if}
{#if isReviewer(task)}
  <span class="q" title={worker ? t('quorum.reviewerTitle', { n: worker.number }) : undefined}>{t('quorum.reviewer')}{worker ? ` #${worker.number}` : ''}</span>
{/if}
{#if sum && task.votes}
  <span class="q {sum.key}" title={t('quorum.needed', { needed: task.votes.needed, voters: task.votes.voters })}>{t(`quorum.${sum.key}` as Key, { approved: sum.approved, voters: sum.voters })} · {t('quorum.round', { n: sum.round })}</span>
{/if}

<style>
  .q {
    --c: var(--text-2);
    display: inline-flex;
    align-items: center;
    flex: none;
    padding: 0 6px;
    border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
    border-radius: 4px;
    font-size: 10.5px;
    line-height: 16px;
    white-space: nowrap;
    color: var(--c);
  }
  .accepted {
    --c: var(--st-done);
  }
  .rejected {
    --c: var(--st-failed);
  }
  .voting {
    --c: var(--st-input);
  }
</style>
