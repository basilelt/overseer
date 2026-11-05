<script lang="ts">
  import { untrack } from 'svelte';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import GitCommit from '@lucide/svelte/icons/git-commit-horizontal';
  import { t } from '../../i18n.svelte';
  import type { ReviewCommit } from '../../review/types';
  import { age } from '../../time';

  let { title, commits, open = false, onsince }: { title: string; commits: ReviewCommit[]; open?: boolean; onsince?: (c: ReviewCommit) => void } = $props();

  let shown = $state(untrack(() => open));
</script>

<section class="commits">
  <button class="head" aria-expanded={shown} onclick={() => (shown = !shown)}>
    <span class="chev" class:open={shown}><ChevronRight size={14} /></span>
    <GitCommit size={14} class="faint" />
    <span>{title}</span>
  </button>
  {#if shown}
    <ul>
      {#each commits as c (c.sha)}
        <li>
          <span class="sha mono">{c.sha.slice(0, 7)}</span>
          <span class="subject" title={c.subject}>{c.subject}</span>
          <span class="end">
            <span class="meta faint">{c.author} · {age(c.at)}</span>
            {#if onsince}<button class="since" onclick={() => onsince(c)}>{t('review.since')}</button>{/if}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .commits {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    margin: 0 0 12px;
    overflow: hidden;
  }
  .head {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
    text-align: left;
  }
  .chev {
    display: inline-flex;
    color: var(--text-3);
    transition: transform var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border);
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px 6px 30px;
    font-size: 12px;
    min-width: 0;
  }
  li + li {
    border-top: 1px solid var(--border-subtle, var(--border));
  }
  .sha {
    color: var(--accent);
    flex: none;
  }
  .subject {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .end {
    flex: none;
    display: grid;
    justify-items: end;
    align-items: center;
  }
  .meta,
  .since {
    grid-area: 1 / 1;
    white-space: nowrap;
  }
  .since {
    color: var(--text-2);
    font-size: 11.5px;
    padding: 2px 6px;
    border-radius: 4px;
    opacity: 0;
  }
  li:hover .since,
  li:focus-within .since {
    opacity: 1;
  }
  li:hover .meta,
  li:focus-within .meta {
    visibility: hidden;
  }
  .since:hover {
    background: var(--surface-hover, var(--surface-active));
    color: var(--text);
  }
  :global(.phone) .meta {
    visibility: hidden;
  }
  :global(.phone) .since {
    opacity: 1;
    min-height: 32px;
  }
</style>
