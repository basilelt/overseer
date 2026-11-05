<script lang="ts">
  import { onMount, tick } from 'svelte';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import House from '@lucide/svelte/icons/house';
  import Eye from '@lucide/svelte/icons/eye';
  import EyeOff from '@lucide/svelte/icons/eye-off';
  import Folder from '@lucide/svelte/icons/folder';
  import FolderGit from '@lucide/svelte/icons/folder-git';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { crumbs, tildePath, type DirListing } from '../../dirs';

  let { start, onpick }: { start: string; onpick: (l: DirListing) => void } = $props();

  let listing = $state<DirListing | null>(null);
  let error = $state('');
  let loading = $state(false);
  let hidden = $state(false);
  let sel = $state(0);
  let list = $state<HTMLElement>();
  let trail = $state<HTMLElement>();
  let seq = 0;
  const parts = $derived(listing ? crumbs(listing.path, listing.home) : []);

  export async function jump(path: string) {
    const n = ++seq;
    loading = true;
    try {
      const l = await app.api.dirs(path, hidden);
      if (n !== seq) return;
      listing = l;
      error = '';
      sel = 0;
      await tick();
      list?.scrollTo?.(0, 0);
      if (document.activeElement === document.body) list?.focus();
      if (trail) trail.scrollLeft = trail.scrollWidth;
    } catch (e) {
      if (n === seq) error = String((e as Error)?.message ?? e);
    } finally {
      if (n === seq) loading = false;
    }
  }

  onMount(() => {
    jump(start);
    list?.focus();
  });

  function move(d: number) {
    const n = listing?.dirs.length ?? 0;
    if (!n) return;
    sel = Math.max(0, Math.min(n - 1, sel + d));
    list?.querySelector(`[data-i="${sel}"]`)?.scrollIntoView?.({ block: 'nearest' });
  }

  function onKey(e: KeyboardEvent) {
    const d = listing?.dirs[sel];
    if (e.key === 'ArrowDown') move(1);
    else if (e.key === 'ArrowUp') move(-1);
    else if (e.key === 'Home') move(-Infinity);
    else if (e.key === 'End') move(Infinity);
    else if (e.key === 'Enter' && d) jump(d.path);
    else if (e.key === 'Backspace' && listing?.parent) jump(listing.parent);
    else return;
    e.preventDefault();
  }
</script>

<div class="browser">
  <div class="bar">
    <button type="button" class="icon-btn" title={t('ws.parentFolder')} aria-label={t('ws.parentFolder')} disabled={!listing?.parent} onclick={() => listing?.parent && jump(listing.parent)}><ArrowUp size={14} /></button>
    <button type="button" class="icon-btn" title={t('ws.homeFolder')} aria-label={t('ws.homeFolder')} onclick={() => jump('~')}><House size={14} /></button>
    <nav class="crumbs mono" bind:this={trail}>
      {#each parts as c, i (c.path)}
        {#if i > 0}<ChevronRight size={11} />{/if}
        <button type="button" class="crumb" aria-current={i === parts.length - 1 ? 'location' : undefined} onclick={() => jump(c.path)}>{c.label}</button>
      {/each}
    </nav>
    <button
      type="button"
      class="icon-btn"
      class:active={hidden}
      title={t('ws.showHidden')}
      aria-label={t('ws.showHidden')}
      aria-pressed={hidden}
      onclick={() => {
        hidden = !hidden;
        jump(listing?.path ?? start);
      }}>{#if hidden}<Eye size={14} />{:else}<EyeOff size={14} />{/if}</button
    >
  </div>
  <div
    class="list"
    class:loading
    role="listbox"
    tabindex="0"
    aria-label={t('ws.subfolders')}
    aria-activedescendant={listing?.dirs.length ? `fb-${sel}` : undefined}
    bind:this={list}
    onkeydown={onKey}
  >
    {#if error}
      <em class="err" role="alert">{error}</em>
    {:else if listing && !listing.dirs.length}
      <span class="faint empty">{t('ws.noSubfolders')}</span>
    {:else if !listing}
      <span class="faint empty">{t('common.loading')}</span>
    {/if}
    {#each listing?.dirs ?? [] as d, i (d.path)}
      <button type="button" class="row" id="fb-{i}" data-i={i} role="option" aria-selected={i === sel} tabindex="-1" onclick={() => jump(d.path)}>
        {#if d.git}<span class="git" title={t('ws.gitRepo')}><FolderGit size={14} /></span>{:else}<Folder size={14} />{/if}
        <span class="name">{d.name}</span>
      </button>
    {/each}
    {#if listing?.truncated}<span class="faint empty">{t('ws.truncated', { n: listing.dirs.length })}</span>{/if}
  </div>
  <div class="foot">
    <span class="cur mono faint" title={listing?.path}>{listing ? tildePath(listing.path, listing.home) : ''}</span>
    <button type="button" class="btn primary" disabled={!listing} onclick={() => listing && onpick(listing)}>{t('ws.useFolder')}</button>
  </div>
</div>

<style>
  .browser {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
    overflow: hidden;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    border-bottom: 1px solid var(--border);
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .crumbs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 1px;
    overflow-x: auto;
    scrollbar-width: none;
    font-size: 11.5px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .crumb {
    flex: none;
    padding: 3px 4px;
    border-radius: 4px;
    color: var(--text-2);
  }
  .crumb:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .crumb[aria-current] {
    color: var(--text);
    font-weight: 600;
  }
  .list {
    display: flex;
    flex-direction: column;
    height: 220px;
    overflow-y: auto;
    padding: 4px;
    outline: none;
  }
  .list:focus-visible {
    box-shadow: inset 0 0 0 2px var(--focus);
  }
  .list.loading {
    opacity: 0.6;
  }
  .row {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    padding: 0 8px;
    border-radius: 4px;
    color: var(--text-2);
    text-align: left;
  }
  .row:hover {
    background: var(--surface-hover);
  }
  .row[aria-selected='true'] {
    background: var(--surface-active);
    color: var(--text);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font-size: 12.5px;
  }
  .git {
    display: inline-grid;
    color: var(--accent);
  }
  .empty,
  .err {
    padding: 8px;
    font-size: 12px;
  }
  .err {
    font-style: normal;
    color: var(--st-failed);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 6px 6px 10px;
    border-top: 1px solid var(--border);
  }
  .cur {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
  }
  :global(.phone) .list {
    height: 36dvh;
  }
  :global(.phone) .row {
    min-height: var(--touch);
  }
  :global(.phone) .name {
    font-size: 15px;
  }
  :global(.phone) .crumbs {
    font-size: 13px;
  }
  :global(.phone) .crumb {
    padding: 8px 6px;
  }
</style>
