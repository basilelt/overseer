<script lang="ts">
  import Circle from '@lucide/svelte/icons/circle';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import type { Todo } from '../../types';

  let { todos }: { todos: Todo[] } = $props();
  const done = $derived(todos.filter((x) => x.state === 'completed').length);
</script>

<div class="bar" role="progressbar" aria-valuemin={0} aria-valuemax={todos.length} aria-valuenow={done}>
  <div class="fill" style:width="{todos.length ? (done / todos.length) * 100 : 0}%"></div>
</div>
<ul>
  {#each todos as td, i (i)}
    <li class={td.state}>
      {#if td.state === 'completed'}<CircleCheck size={13} />{:else if td.state === 'in_progress'}<LoaderCircle size={13} />{:else}<Circle
          size={13}
        />{/if}
      <span>{td.text}</span>
    </li>
  {/each}
</ul>

<style>
  .bar {
    height: 3px;
    border-radius: 2px;
    background: var(--surface-2);
    overflow: hidden;
    margin: 2px 0 8px;
  }
  .fill {
    height: 100%;
    background: var(--st-working);
    transition: width 300ms ease;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 3px 0;
    font-size: 12px;
    color: var(--text-3);
  }
  li :global(svg) {
    margin-top: 2px;
    flex: none;
  }
  li span {
    color: var(--text);
  }
  .completed span {
    color: var(--text-3);
    text-decoration: line-through;
  }
  .in_progress {
    color: var(--st-working);
  }
</style>
