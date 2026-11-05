<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import Search from '@lucide/svelte/icons/search';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { fillFrom, filterItems, sourcesApi } from '../../task-sources';
  import type { SourceItems, TaskSourceKind } from '../../types';

  const NAMES: Record<TaskSourceKind, string> = { github: 'GitHub', gitlab: 'GitLab', jira: 'Jira' };

  let { project, onpick }: { project: string; onpick: (fill: ReturnType<typeof fillFrom>) => void } = $props();

  const api = sourcesApi(app.client!);
  let data = $state<SourceItems | null>(null);
  let loading = $state(false);
  let error = $state('');
  let query = $state('');
  let kind = $state<TaskSourceKind>('github');
  let target = $state('');
  let adding = $state(false);

  const shown = $derived(data ? filterItems(data.items, query) : []);
  const failed = $derived(new Map((data?.errors ?? []).map((e) => [e.source, e.error])));

  let seq = 0;

  async function load(p: string) {
    const n = ++seq;
    loading = true;
    error = '';
    try {
      const got = await api.items(p);
      if (n === seq) data = got;
    } catch (e) {
      if (n === seq) error = String((e as Error)?.message ?? e);
    } finally {
      if (n === seq) loading = false;
    }
  }

  $effect(() => {
    data = null;
    load(project);
  });

  async function add() {
    if (!target.trim() || adding) return;
    adding = true;
    error = '';
    try {
      await api.add(project, kind, target.trim());
      target = '';
      await load(project);
    } catch (e) {
      error = String((e as Error)?.message ?? e);
    } finally {
      adding = false;
    }
  }

  async function remove(id: string) {
    try {
      await api.remove(id);
      await load(project);
    } catch (e) {
      error = String((e as Error)?.message ?? e);
    }
  }
</script>

<div class="sp" aria-label={t('task.import')} role="group">
  <div class="search">
    <Search size={13} />
    <input bind:value={query} placeholder={t('task.importFilter')} aria-label={t('task.importFilter')} onkeydown={(e) => e.key === 'Enter' && (e.preventDefault(), shown[0] && onpick(fillFrom(shown[0])))} />
    <button type="button" class="icon" aria-label={t('common.retry')} title={t('common.retry')} disabled={loading} onclick={() => load(project)}><RefreshCw size={13} /></button>
  </div>
  <div class="options" role="listbox" aria-busy={loading}>
    {#if loading && !data}
      <div class="none faint">{t('common.loading')}</div>
    {:else}
      {#each shown as i (i.source + i.key)}
        <button type="button" role="option" aria-selected="false" class="opt" onclick={() => onpick(fillFrom(i))}>
          <span class="mono num">{i.key}</span>
          <span class="name">{i.title}</span>
        </button>
      {:else}
        <div class="none faint">{t('common.none')}</div>
      {/each}
    {/if}
  </div>
  {#if error}<em class="err" role="alert">{error}</em>{/if}
  <div class="sources">
    <span class="label">{t('task.sources')}</span>
    {#each data?.sources ?? [] as s (s.id)}
      <div class="src" class:bad={failed.has(s.id)} title={failed.get(s.id) ?? ''}>
        <span class="kind">{NAMES[s.kind]}</span>
        <span class="mono target">{s.target}</span>
        {#if s.implicit}
          <span class="faint tag">origin</span>
        {:else}
          <button type="button" class="icon" aria-label={t('common.delete')} onclick={() => remove(s.id)}><X size={12} /></button>
        {/if}
      </div>
      {#if failed.has(s.id)}<em class="err">{failed.get(s.id)}</em>{/if}
    {/each}
    <div class="add">
      <select bind:value={kind} aria-label={t('task.sources')}>
        {#each Object.entries(NAMES) as [k, n] (k)}<option value={k}>{n}</option>{/each}
      </select>
      <input class="mono" bind:value={target} placeholder={kind === 'jira' ? 'JQL' : kind === 'gitlab' ? 'group/repo' : 'owner/repo'} aria-label={t('task.sourceAdd')} spellcheck="false" onkeydown={(e) => e.key === 'Enter' && (e.preventDefault(), add())} />
      <button type="button" class="btn" disabled={!target.trim() || adding} onclick={add}>{t('task.sourceAdd')}</button>
    </div>
  </div>
</div>

<style>
  .sp {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .search,
  .add,
  .src {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .search {
    color: var(--text-3);
  }
  .search > input,
  .add > input {
    flex: 1;
    min-width: 0;
  }
  .add > select {
    flex: none;
    width: auto;
  }
  .add > .btn {
    flex: none;
  }
  .options {
    display: flex;
    flex-direction: column;
    max-height: 200px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 8px;
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
  .name,
  .target {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .mono {
    font-size: 11px;
    color: var(--text-3);
  }
  .num,
  .kind,
  .tag {
    flex: none;
  }
  .none {
    padding: 8px;
    font-size: 12px;
  }
  .sources {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .label {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .src {
    font-size: 12px;
  }
  .src.bad .kind {
    color: var(--st-failed);
  }
  .tag {
    margin-left: auto;
    font-size: 11px;
  }
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 22px;
    height: 22px;
    margin-left: auto;
    border-radius: 3px;
    color: var(--text-3);
  }
  .icon:hover {
    color: var(--text);
    background: var(--surface-hover);
  }
  .err {
    font-style: normal;
    font-size: 12px;
    color: var(--st-failed);
    overflow-wrap: anywhere;
  }
  :global(.phone) .add {
    flex-wrap: wrap;
  }
</style>
