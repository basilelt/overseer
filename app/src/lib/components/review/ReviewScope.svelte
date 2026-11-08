<script lang="ts">
  import FolderGit from '@lucide/svelte/icons/folder-git-2';
  import { t } from '../../i18n.svelte';
  import { parseReviewKey } from '../../review/keys';
  import { openReview, openReviewTabs } from '../../review/open';
  import { changedLabel, changedWorktrees, counts, folder, includeCurrent, scopeOptions, type ScopeOption } from '../../review/scope';
  import type { ReviewStore } from '../../review/store.svelte';
  import { age } from '../../time';

  let { store, selfKey }: { store: ReviewStore; selfKey: string } = $props();

  const opts = $derived(
    includeCurrent(
      scopeOptions(store.related, selfKey, openReviewTabs()),
      selfKey,
      store.target.branch ?? store.diff?.branch ?? folder(store.worktree ?? ''),
      store.worktree ?? '',
    ),
  );
  const changedList = $derived(changedWorktrees(store.related));

  function label(o: ScopeOption): string {
    const parts = [o.name];
    const w = o.worktree;
    if (w) {
      parts.push(counts(w), age(w.at));
      if (w.current) parts.push(t('review.current'));
      else if (w.branch && folder(w.path) !== w.branch.split('/').pop()) parts.push(folder(w.path));
    }
    return parts.filter(Boolean).join(' · ');
  }

  function pick(key: string) {
    if (key === selfKey) return;
    const target = parseReviewKey(key);
    if (target) openReview(target);
  }
</script>

{#if opts.length > 1}
  <div class="scope">
    <FolderGit size={14} class="faint" />
    <span class="count">{changedLabel(changedList)}</span>
    <select aria-label={t('review.scope')} value={selfKey} onchange={(e) => pick(e.currentTarget.value)}>
      <optgroup label={t('review.scopeWorktrees')}>
        {#each opts as o (o.key)}<option value={o.key}>{label(o)}</option>{/each}
      </optgroup>
    </select>
  </div>
{/if}

<style>
  .scope {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px 6px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
    font-size: 12px;
    min-width: 0;
  }
  .count {
    color: var(--text-2);
    white-space: nowrap;
    font-weight: 500;
  }
  select {
    flex: 1;
    min-width: 0;
    max-width: 640px;
    height: 28px;
    padding: 2px 8px;
    font-size: 12px;
    color: var(--text);
  }
  :global(.phone) .scope {
    flex-wrap: wrap;
    padding: 8px 12px;
  }
  :global(.phone) select {
    flex: 1 1 100%;
    max-width: none;
    height: 44px;
    font-size: 14px;
  }
</style>
