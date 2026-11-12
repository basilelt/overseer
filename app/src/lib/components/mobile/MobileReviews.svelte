<script lang="ts">
  import GitCompare from '@lucide/svelte/icons/git-compare';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { allTabs, workspace } from '../../workspace/workspace.svelte';
  import { openReview, reviewTitle, testingReviews } from '../../review/open';
  import { isReviewKey, reviewKey } from '../../review/keys';
  import ReviewBadge from '../review/ReviewBadge.svelte';

  const testing = $derived(testingReviews());
  const testingKeys = $derived(new Set(testing.map((x) => reviewKey({ taskId: x.id }))));
  const open = $derived(allTabs(workspace.layout).filter((k) => isReviewKey(k) && !testingKeys.has(k)));

  function show(key: string) {
    app.view = 'agents';
    app.selectedKey = key;
  }
</script>

<div class="reviews">
  {#if testing.length}
    <h3>{t('mobile.shell.inTesting')}</h3>
    <ul>
      {#each testing as task (task.id)}
        <li>
          <button class="row" onclick={() => openReview({ taskId: task.id })}>
            <span class="dot" style:background="var(--st-testing)"></span>
            <span class="text">
              <span class="title">{task.title}</span>
              <span class="sub faint">{t('mobile.shell.task', { n: task.number })}{task.branch ? ` · ${task.branch}` : ''}</span>
            </span>
            <ReviewBadge tabKey={reviewKey({ taskId: task.id })} />
            <ChevronRight size={16} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if open.length}
    <h3>{t('mobile.shell.openReviews')}</h3>
    <ul>
      {#each open as key (key)}
        <li>
          <button class="row" onclick={() => show(key)}>
            <GitCompare size={15} />
            <span class="text"><span class="title">{reviewTitle(key) ?? key}</span></span>
            <ReviewBadge tabKey={key} />
            <ChevronRight size={16} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if !testing.length && !open.length}
    <div class="empty">
      <GitCompare size={28} strokeWidth={1.5} />
      <p class="faint">{t('mobile.shell.reviewsEmpty')}</p>
    </div>
  {/if}
</div>

<style>
  .reviews {
    flex: 1;
    overflow-y: auto;
    padding: 8px 0 16px;
  }
  h3 {
    margin: 12px 16px 4px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 56px;
    padding: 8px 16px;
    text-align: left;
    border-bottom: 1px solid var(--border);
    color: var(--text-2);
  }
  .row:active {
    background: var(--surface-active);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .title {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    font-size: 12.5px;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 64px 32px;
    text-align: center;
    color: var(--text-3);
  }
</style>
