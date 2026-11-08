<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import Search from '@lucide/svelte/icons/search';
  import { app, projectName } from '../../state.svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { depCandidates, isDone } from '../../kanban';
  import type { Task } from '../../types';

  let {
    value,
    project,
    self,
    inherit,
    onchange,
    oninherit,
  }: {
    value: string[];
    project: string;
    self?: string;
    inherit?: boolean | null;
    onchange: (ids: string[]) => void;
    oninherit: (on: boolean) => void;
  } = $props();

  let query = $state('');
  let open = $state(false);
  let everywhere = $state(false);

  const chosen = $derived(value.map((id) => app.data.tasks.find((x) => x.id === id)).filter((x): x is Task => !!x));
  const options = $derived(open ? depCandidates(app.data.tasks, query, { project: everywhere ? undefined : project, exclude: self ? [...value, self] : value }) : []);

  function add(task: Task) {
    onchange([...value, task.id]);
    query = '';
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && options.length) {
      e.preventDefault();
      e.stopPropagation();
      add(options[0]);
    } else if (e.key === 'Escape' && (query || open)) {
      e.stopPropagation();
      query = '';
      open = false;
    }
  }
</script>

<div class="picker">
  {#if chosen.length}
    <div class="chips">
      {#each chosen as d (d.id)}
        <span class="chip" class:done={isDone(d.status)} title={t(`kanban.col.${d.status}` as Key)}>
          <span class="dot {d.status}"></span>
          <span class="mono">#{d.number}</span>
          <span class="name">{d.title}</span>
          <button type="button" class="x" aria-label={t('task.removeDep', { n: d.number })} onclick={() => onchange(value.filter((id) => id !== d.id))}><X size={12} /></button>
        </span>
      {/each}
    </div>
  {/if}
  <div class="search">
    <Search size={13} />
    <input
      bind:value={query}
      placeholder={t('task.depsSearch')}
      aria-label={t('task.dependsOn')}
      role="combobox"
      aria-expanded={open}
      aria-controls="dep-options"
      onfocus={() => (open = true)}
      onblur={() => (open = false)}
      onkeydown={onKey}
    />
    <label class="all">
      <input type="checkbox" bind:checked={everywhere} />
      <span>{t('task.depsAllProjects')}</span>
    </label>
  </div>
  {#if open}
    <div class="options" id="dep-options" role="listbox">
      {#each options as o (o.id)}
        <button type="button" role="option" aria-selected="false" class="opt" onpointerdown={(e) => e.preventDefault()} onclick={() => add(o)}>
          <span class="dot {o.status}"></span>
          <span class="mono num">#{o.number}</span>
          <span class="name">{o.title}</span>
          {#if everywhere && o.project !== project}<span class="proj faint">{projectName(app.allProjects, o.project)}</span>{/if}
        </button>
      {:else}
        <div class="none faint">{t('task.depsNone')}</div>
      {/each}
    </div>
  {/if}
  {#if chosen.length === 1}
    <label class="tog">
      <input type="checkbox" class="switch" checked={inherit !== false} onchange={(e) => oninherit(e.currentTarget.checked)} />
      <span>{t('task.inheritBranch', { n: chosen[0].number })}</span>
    </label>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    padding: 1px 2px 1px 7px;
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 12px;
    background: var(--surface-2);
    min-width: 0;
  }
  .chip.done {
    color: var(--text-3);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .mono {
    font-size: 11px;
    color: var(--text-3);
    flex: none;
  }
  .x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 3px;
    color: var(--text-3);
  }
  .x:hover {
    color: var(--text);
    background: var(--surface-hover);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-3);
  }
  .search > input {
    flex: 1;
    min-width: 0;
  }
  .all {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
    font-size: 11.5px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .options {
    display: flex;
    flex-direction: column;
    max-height: 180px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 30px;
    padding: 4px 8px;
    text-align: left;
    font-size: 12.5px;
    color: var(--text);
    min-width: 0;
  }
  .opt:hover {
    background: var(--surface-hover);
  }
  .proj {
    margin-left: auto;
    flex: none;
    font-size: 11px;
  }
  .none {
    padding: 8px;
    font-size: 12px;
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .dot.in_progress {
    background: var(--st-working);
  }
  .dot.in_testing {
    background: var(--st-testing);
  }
  .dot.merged {
    background: var(--st-input);
  }
  .dot.completed {
    background: var(--st-done);
  }
  .tog {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  :global(.phone) .opt {
    min-height: 44px;
  }
  :global(.phone) .x {
    width: 32px;
    height: 32px;
  }
</style>
