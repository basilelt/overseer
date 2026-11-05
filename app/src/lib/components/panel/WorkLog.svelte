<script lang="ts">
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleDashed from '@lucide/svelte/icons/circle-dashed';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { age } from '../../time';
  import type { WorkStep } from '../../types';

  let { steps }: { steps: WorkStep[] } = $props();
</script>

<ol class="log">
  {#each steps as s, i (i)}
    <li class={s.state}>
      <span class="icon">
        {#if s.state === 'running'}<LoaderCircle size={13} />{:else if s.state === 'done'}<CircleCheck size={13} />{:else if s.state === 'failed'}<CircleX
            size={13}
          />{:else}<CircleDashed size={13} />{/if}
      </span>
      <span class="label" title={s.path ?? s.label}>{s.label}</span>
      <span class="at faint">{age(s.at)}</span>
    </li>
  {/each}
</ol>

<style>
  .log {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
    font-size: 12px;
  }
  .icon {
    display: inline-grid;
    color: var(--text-3);
  }
  .running .icon {
    color: var(--st-working);
  }
  .failed .icon {
    color: var(--st-failed);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .done .label {
    color: var(--text-2);
  }
  .at {
    font-size: 11px;
  }
</style>
