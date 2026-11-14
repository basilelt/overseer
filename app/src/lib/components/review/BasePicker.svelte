<script lang="ts">
  import { dismiss } from '../../dismiss';
  import Check from '@lucide/svelte/icons/check';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { t, type Key } from '../../i18n.svelte';
  import { isSince } from '../../review/base';
  import type { ReviewStore } from '../../review/store.svelte';
  import type { BaseRef, Bases } from '../../review/types';
  import { age } from '../../time';

  let { store }: { store: ReviewStore } = $props();

  let open = $state(false);
  let query = $state('');
  let bases = $state.raw<Bases | null>(null);
  let failed = $state('');
  let root: HTMLDivElement | undefined = $state();
  let input: HTMLInputElement | undefined = $state();
  let btn: HTMLButtonElement | undefined = $state();
  let at = $state({ top: 0, left: 0, width: 340 });

  const diff = $derived(store.diff);
  const current = $derived(store.base && !isSince(store.base) ? store.base : null);
  const label = $derived(current ?? diff?.base ?? 'HEAD');
  const source = $derived(current ? 'explicit' : (diff?.baseSource ?? null));
  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = bases?.branches ?? [];
    return q ? list.filter((b) => b.name.toLowerCase().includes(q)) : list;
  });
  const local = $derived(matches.filter((b) => !b.remote));
  const remote = $derived(matches.filter((b) => b.remote));

  async function toggle() {
    open = !open;
    if (!open) return;
    const r = btn?.getBoundingClientRect();
    const width = Math.min(340, innerWidth - 24);
    if (r) at = { top: r.bottom + 4, left: Math.max(12, Math.min(r.left, innerWidth - width - 12)), width };
    query = '';
    setTimeout(() => input?.focus(), 0);
    if (bases) return;
    try {
      const target = store.worktree ? { worktree: store.worktree } : store.target;
      bases = await store.api.bases(target);
      failed = '';
    } catch (e) {
      failed = String((e as Error)?.message ?? e);
    }
  }

  function pick(name: string | null) {
    open = false;
    store.setBase(name, true);
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') open = false;
    else if (e.key === 'Enter' && matches[0]) pick(matches[0].name);
  }

</script>

{#snippet item(b: BaseRef)}
  <button class="opt" class:on={b.name === current} onclick={() => pick(b.name)} title={b.subject}>
    <span class="mono name">{b.name}</span>
    <span class="faint when">{age(b.at)}</span>
    {#if b.name === current}<Check size={12} />{/if}
  </button>
{/snippet}

<div class="picker" bind:this={root} use:dismiss={{ active: open, onclose: () => (open = false) }}>
  <button class="cur mono" bind:this={btn} aria-haspopup="listbox" aria-expanded={open} title={source ? t(`review.source.${source}` as Key, { base: label }) : t('review.pickBase')} onclick={toggle}>
    <span class="lbl">{label}</span><ChevronDown size={12} />
  </button>
  {#if open}
    <div class="drop" role="listbox" style:top="{at.top}px" style:left="{at.left}px" style:width="{at.width}px">
      <input bind:this={input} bind:value={query} placeholder={t('review.findBase')} aria-label={t('review.findBase')} onkeydown={key} />
      <div class="list">
        <button class="opt" class:on={!current} onclick={() => pick(null)}>
          <span class="name">{t('review.defaultBase', { base: bases?.base ?? bases?.default ?? diff?.base ?? 'HEAD' })}</span>
          {#if !current}<Check size={12} />{/if}
        </button>
        {#if !bases && !failed}
          <div class="msg faint"><LoaderCircle size={12} class="spin" />{t('common.loading')}</div>
        {:else if failed}
          <div class="msg err">{failed}</div>
        {:else}
          {#if local.length}<div class="grp">{t('review.localBranches')}</div>{/if}
          {#each local as b (b.name)}{@render item(b)}{/each}
          {#if remote.length}<div class="grp">{t('review.remoteBranches')}</div>{/if}
          {#each remote as b (b.name)}{@render item(b)}{/each}
          {#if !matches.length}<div class="msg faint">{t('review.noBase')}</div>{/if}
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
    min-width: 0;
    display: inline-flex;
  }
  .cur {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    max-width: 260px;
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
  }
  .cur:hover {
    border-color: var(--accent);
  }
  .lbl {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .drop {
    position: fixed;
    z-index: 30;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow-lg, 0 8px 24px rgb(0 0 0 / 0.18));
  }
  input {
    margin: 8px;
    height: 28px;
    padding: 2px 8px;
    font-size: 12px;
  }
  .list {
    max-height: min(320px, 50vh);
    overflow: auto;
    padding: 0 4px 6px;
  }
  .grp {
    padding: 8px 8px 2px;
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.02em;
    color: var(--text-3);
  }
  .opt {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: 4px;
    font-size: 12px;
    color: var(--text);
    text-align: left;
  }
  .opt:hover {
    background: var(--surface-hover);
  }
  .opt.on {
    color: var(--accent);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    flex: none;
    font-size: 11px;
  }
  .msg {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px;
    font-size: 12px;
  }
  .err {
    color: var(--st-failed);
  }
  :global(.phone) input {
    height: 40px;
    font-size: 14px;
  }
  :global(.phone) .opt {
    min-height: 40px;
  }
</style>
