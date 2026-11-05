<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import MessageSquare from '@lucide/svelte/icons/message-square';
  import { t } from '../../i18n.svelte';
  import { fileTree, type TreeNode } from '../../review/rows';
  import { byRisk } from '../../review/triage';
  import type { ReviewFile } from '../../review/types';
  import Self from './FileTree.svelte';

  let {
    files,
    nodes,
    counts,
    selected,
    depth = 0,
    flat = false,
    onselect,
  }: { files?: ReviewFile[]; nodes?: TreeNode[]; counts: Record<string, number>; selected: string | null; depth?: number; flat?: boolean; onselect: (path: string) => void } = $props();

  const STATUS: Record<string, string> = { added: 'A', modified: 'M', deleted: 'D', renamed: 'R', copied: 'C', untracked: 'U', submodule: 'S' };
  const list = $derived(nodes ?? (flat ? byRisk(files ?? []).map((f) => ({ name: f.path, path: f.path, dir: false, file: f, children: [] })) : fileTree(files ?? [])));
  const level = (r: number) => (r >= 60 ? 'hi' : r >= 25 ? 'mid' : 'lo');
  let closed = $state<Record<string, boolean>>({});
</script>

<ul class="tree" class:root={depth === 0}>
  {#each list as n (n.path + (n.dir ? '/' : ''))}
    <li>
      {#if n.dir}
        <button class="row dir" style:padding-left="{8 + depth * 12}px" onclick={() => (closed[n.path] = !closed[n.path])} aria-expanded={!closed[n.path]}>
          <span class="chev" class:open={!closed[n.path]}><ChevronRight size={12} /></span>
          <span class="name">{n.name}</span>
        </button>
        {#if !closed[n.path]}
          <Self nodes={n.children} {counts} {selected} depth={depth + 1} {onselect} />
        {/if}
      {:else if n.file}
        <button class="row" class:active={selected === n.path} class:viewed={n.file.viewed} style:padding-left="{22 + depth * 12}px" title={n.path} onclick={() => onselect(n.path)}>
          <span class="st st-{n.file.status}">{STATUS[n.file.status]}</span>
          <span class="name">{n.name}</span>
          {#if n.file.risk !== undefined && (n.file.residual ?? 0) > 0}<span class="risk {level(n.file.risk)}" title={t('review.risk', { n: n.file.risk })}>{n.file.risk}</span>{/if}
          {#if counts[n.path]}<span class="cc"><MessageSquare size={11} />{counts[n.path]}</span>{/if}
          {#if n.file.viewed}<span class="ok"><Check size={12} /></span>{/if}
        </button>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .tree {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .root {
    padding: 6px 6px 12px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 26px;
    padding-right: 8px;
    border-radius: var(--radius);
    color: var(--text-2);
    font-size: 12px;
    text-align: left;
  }
  .row:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .row.active {
    background: var(--surface-active);
    color: var(--text);
  }
  .row.viewed .name {
    color: var(--text-3);
  }
  .dir {
    color: var(--text-2);
  }
  .chev {
    display: inline-grid;
    color: var(--text-3);
    transition: transform var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .st {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 600;
    width: 10px;
    flex: none;
  }
  .st-added,
  .st-untracked {
    color: var(--diff-add);
  }
  .st-deleted {
    color: var(--diff-del);
  }
  .st-modified {
    color: #b45309;
  }
  :global([data-theme='dark']) .st-modified {
    color: #fbbf24;
  }
  .st-renamed,
  .st-submodule {
    color: var(--diff-hunk);
  }
  .cc {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    color: var(--accent);
  }
  .ok {
    display: inline-grid;
    color: var(--diff-add);
  }
  .risk {
    flex: none;
    min-width: 22px;
    padding: 0 4px;
    border-radius: 4px;
    font-family: var(--mono);
    font-size: 10px;
    line-height: 16px;
    text-align: center;
    color: var(--text-2);
    background: var(--surface-active);
  }
  .risk.hi {
    color: var(--st-failed);
    background: color-mix(in srgb, var(--st-failed) 14%, transparent);
  }
  .risk.mid {
    color: var(--text);
    background: color-mix(in srgb, var(--lane-3) 22%, transparent);
  }
</style>
