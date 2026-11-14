<script lang="ts">
  import Modal from '../components/ui/Modal.svelte';
  import { t } from '../i18n.svelte';
  import { app } from '../state.svelte';
  import { gitApi } from './api';
  import { SvelteSet } from 'svelte/reactivity';
  import { chosen, cleanedMessage, cleanupTargets, planLines, runCleanup } from './cleanup';
  import { gt } from './strings';
  import type { CleanupPlan, Worktree } from './types';

  let { worktrees, onclose, ondone }: { worktrees: Worktree[]; onclose: () => void; ondone?: () => void } = $props();

  let plans = $state<CleanupPlan[] | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const skipped = new SvelteSet<string>();
  const runnable = $derived(cleanupTargets(plans ?? []));
  const picked = $derived(chosen(runnable, skipped));
  const toggle = (id: string) => (skipped.has(id) ? skipped.delete(id) : skipped.add(id));
  const byId = $derived(new Map(worktrees.map((w) => [w.id, w])));

  $effect(() => {
    const api = gitApi(app.requireClient());
    Promise.all(worktrees.map((w) => api.cleanup(w.id, true, w.project)))
      .then((p) => (plans = p))
      .catch((e) => (error = String((e as Error)?.message ?? e)));
  });

  async function confirm() {
    busy = true;
    const r = await runCleanup(gitApi(app.requireClient()), picked, (id) => byId.get(id)?.project);
    busy = false;
    if (r.failed.length) app.notify(r.failed.map((f) => f.error).join(' · '), 'error');
    if (r.done.length) app.notify(cleanedMessage(r.done.length));
    ondone?.();
    onclose();
  }
</script>

<Modal title={gt('cleanupTitle')} {onclose} width={620}>
  <p class="intro faint">{gt('cleanupIntro')}</p>
  {#if error}
    <p class="err">{error}</p>
  {:else if !plans}
    <p class="faint">{t('common.loading')}</p>
  {:else}
    <ul class="plans">
      {#each plans as p (p.id)}
        {@const w = byId.get(p.id)}
        <li class:refused={!!p.refused}>
          <label class="title">
            {#if runnable.includes(p.id)}<input type="checkbox" aria-label={gt('pick')} checked={!skipped.has(p.id)} disabled={busy} onchange={() => toggle(p.id)} />{/if}
            <span class="names">
              <span class="mono branch">{w?.branch ?? '—'}</span>
              <span class="mono faint path">{p.id}</span>
            </span>
          </label>
          {#each planLines(p) as l, i (i)}
            <div class="line {l.kind}">
              <span class="tag">{l.kind === 'remove' ? '−' : l.kind === 'keep' ? '=' : '!'}</span>
              <span>{l.label}</span>
              {#if l.detail}<span class="mono faint">{l.detail}</span>{/if}
            </div>
          {/each}
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t('common.cancel')}</button>
    <button class="btn danger" disabled={busy || !picked.length} onclick={confirm}>
      {runnable.length ? gt('confirmCleanup', { n: picked.length }) : gt('nothingToClean')}
    </button>
  {/snippet}
</Modal>

<style>
  .intro {
    margin: 0 0 12px;
  }
  .plans {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .title {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin-bottom: 6px;
  }
  .title input {
    margin-top: 2px;
  }
  .names {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .branch {
    font-size: 12.5px;
    font-weight: 500;
  }
  .path {
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .line {
    display: flex;
    gap: 8px;
    align-items: baseline;
    font-size: 12.5px;
    line-height: 1.7;
  }
  .tag {
    width: 12px;
    font-family: var(--mono);
    color: var(--text-3);
  }
  .line.remove .tag {
    color: var(--st-failed);
  }
  .line.refused {
    color: var(--text-2);
  }
  .line.refused .tag {
    color: var(--st-failed);
  }
  .err {
    color: var(--st-failed);
  }
</style>
