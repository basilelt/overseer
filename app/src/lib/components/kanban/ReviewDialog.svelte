<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import X from '@lucide/svelte/icons/x';
  import GitCommit from '@lucide/svelte/icons/git-commit-horizontal';
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import Modal from '../ui/Modal.svelte';
  import { openReview } from '../../review/open';
  import { projectName, app } from '../../state.svelte';
  import { t, type Key } from '../../i18n.svelte';

  let { onclose }: { onclose: () => void } = $props();

  type Action = 'approve' | 'reject' | 'commit';

  const reviewView = () => import('../review/ReviewView.svelte');

  const task = $derived(app.data.tasks.find((x) => x.id === app.overlayArg));
  const reviewable = $derived(!!task && !!(task.lineId || task.worktreePath));
  let busy = $state<Action | null>(null);
  let approvePr = $state(false);
  let merge = $state(false);

  async function review(action: Action) {
    if (!task) return;
    busy = action;
    try {
      const updated = await app.api.reviewTask(task.id, action, undefined, action === 'approve' ? { approvePr, merge } : {});
      app.data = { ...app.data, tasks: app.data.tasks.map((x) => (x.id === updated.id ? updated : x)) };
      app.notify(t('action.done', { action: t(`review.${action}` as Key) }));
      onclose();
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      busy = null;
    }
  }

  function submitted(kind: string) {
    if (kind === 'request_changes') onclose();
  }
</script>

<Modal title={task ? t('review.title', { n: task.number }) : t('common.error')} {onclose} width={1280} fill>
  {#if task}
    <div class="head">
      <div class="title">{task.title}</div>
      <div class="meta muted">
        <span>{projectName(app.allProjects, task.project)}</span>
        {#if task.branch}<span class="mono">{task.branch}</span>{/if}
        {#if task.ticketKey}<span class="mono">{task.ticketKey}</span>{/if}
        <span>{task.agent}{task.model ? ` · ${task.model}` : ''}</span>
        <span class="spacer"></span>
        {#if reviewable}
          <button class="btn ghost small" onclick={() => openReview({ taskId: task.id })}><Maximize2 size={12} />{t('review.open')}</button>
        {/if}
      </div>
    </div>
    {#if !reviewable}
      <div class="state faint">{t('review.noLine')}</div>
    {:else}
      <div class="frame">
        {#await reviewView() then { default: ReviewView }}<ReviewView target={{ taskId: task.id }} embedded onsubmitted={submitted} />{/await}
      </div>
    {/if}
  {/if}
  {#snippet footer()}
    <button class="btn danger" disabled={!task || !!busy} onclick={() => review('reject')}><X size={14} />{t('review.reject')}</button>
    <span class="spacer"></span>
    <label class="opt" title={t('review.prHint')}><input type="checkbox" bind:checked={approvePr} />{t('review.approvePr')}</label>
    <label class="opt" title={t('review.prHint')}><input type="checkbox" bind:checked={merge} />{t('review.mergePr')}</label>
    <button class="btn" disabled={!task || !!busy} onclick={() => review('commit')}><GitCommit size={14} />{t('review.commit')}</button>
    <button class="btn primary" disabled={!task || !!busy} onclick={() => review('approve')}><Check size={14} />{t('review.approve')}</button>
  {/snippet}
</Modal>

<style>
  .head {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
  }
  .title {
    font-size: 14px;
    font-weight: 600;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 12px;
  }
  .small {
    height: 24px;
    font-size: 12px;
  }
  .frame {
    display: flex;
    flex: 1 1 0;
    min-height: 320px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  :global(.phone) .frame {
    min-height: 470px;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 24px 0;
    justify-content: center;
  }
  .spacer {
    flex: 1;
  }
  .opt {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
  }
</style>
