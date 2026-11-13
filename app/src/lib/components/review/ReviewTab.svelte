<script lang="ts">
  import { parseReviewKey } from '../../review/keys';
  import type { ReviewTarget } from '../../review/types';
  import type { TabInfo } from '../../tabs/tabs';
  import ReviewView from './ReviewView.svelte';
  import { beside } from '../../review/beside.svelte';

  let { tabKey, tab, lineId, worktreePath, taskId }: { tabKey?: string; tab?: TabInfo; focused?: boolean; lineId?: string; worktreePath?: string; taskId?: string } = $props();

  const key = $derived(tabKey ?? tab?.key);
  const target = $derived<ReviewTarget | null>(
    key ? parseReviewKey(key) : worktreePath ? { worktree: worktreePath } : lineId ? { lineId } : taskId ? { taskId } : null,
  );
  const id = $derived(JSON.stringify(target));
</script>

{#if target}
  {#key id}
    {#if key && beside.key === key}
      <ReviewView {target} pinned={beside.pinned} onpin={() => (beside.pinned = !beside.pinned)} />
    {:else}
      <ReviewView {target} />
    {/if}
  {/key}
{/if}
