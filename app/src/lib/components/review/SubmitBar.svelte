<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import MessageSquare from '@lucide/svelte/icons/message-square';
  import Send from '@lucide/svelte/icons/send';
  import { agentName } from '../../agents';
  import { t } from '../../i18n.svelte';
  import type { ReviewStore } from '../../review/store.svelte';
  import { canSubmit } from '../../review/threads';
  import { batchAll, submitAll } from '../../review/linked.svelte';
  import type { Delivery, SubmitKind } from '../../review/types';
  import { app } from '../../state.svelte';

  let { store, linked = [], approve = true, onsubmitted }: { store: ReviewStore; linked?: ReviewStore[]; approve?: boolean; onsubmitted?: (kind: SubmitKind) => void } = $props();

  let body = $state('');
  let expanded = $state(false);
  const b = $derived(batchAll(store, linked));
  const busy = $derived(!!store.submitting || linked.some((s) => !!s.submitting));

  function targetName(d: Delivery): string {
    const tg = d.target ?? '';
    if (tg.startsWith('term:')) {
      const term = app.data.terms.find((x) => x.id === tg.slice(5));
      if (term) return `${agentName(app.agents, term.agent ?? 'shell')} · ${term.title}`;
    }
    if (tg.startsWith('tmux:')) return `tmux ${tg.slice(5)}`;
    return tg || '—';
  }

  async function submit(kind: SubmitKind) {
    const r = await submitAll(store, linked, kind, body);
    if (!r) return;
    body = '';
    expanded = false;
    onsubmitted?.(kind);
  }
</script>

<div class="bar">
  {#if expanded}
    <textarea bind:value={body} rows="2" placeholder={t('review.summary')} spellcheck="true"></textarea>
  {/if}
  <div class="row">
    <button class="pending" class:has={b.comments > 0} onclick={() => (expanded = !expanded)} title={b.files.map((f) => `${f.path} (${f.count})`).join('\n')}>
      <MessageSquare size={13} />
      {b.comments ? t('review.pending', { n: b.comments }) : t('review.noPending')}
    </button>
    {#if store.delivery}
      {@const d = store.delivery}
      <span class="delivery {d.status}">
        {#if d.status === 'queued'}<LoaderCircle size={12} class="spin" />{t('review.queued', { reason: d.reason || t('review.busy') })}
        {:else if d.status === 'delivered'}<Check size={12} />{t('review.delivered', { target: targetName(d) })}
        {:else}{d.reason || t('review.nothing')}{/if}
      </span>
    {:else}
      <span class="hint faint">{t('review.sendHint')}</span>
    {/if}
    <span class="spacer"></span>
    <button class="btn" disabled={busy || !canSubmit('comment', b, body)} onclick={() => submit('comment')}>{t('review.kind.comment')}</button>
    {#if approve}<button class="btn" disabled={busy} onclick={() => submit('approve')}><Check size={13} />{t('review.kind.approve')}</button>{/if}
    <button class="btn primary" disabled={busy || !canSubmit('request_changes', b, body)} onclick={() => submit('request_changes')}>
      {#if store.submitting === 'request_changes'}<LoaderCircle size={13} class="spin" />{:else}<Send size={13} />{/if}{t('review.kind.request_changes')}
    </button>
  </div>
</div>

<style>
  .bar {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  textarea {
    width: 100%;
    resize: vertical;
    font-size: 12.5px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .pending {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border-radius: var(--radius);
    color: var(--text-2);
    font-size: 12px;
    white-space: nowrap;
  }
  .pending:hover {
    background: var(--surface-hover);
  }
  .pending.has {
    color: var(--text);
    font-weight: 500;
  }
  .hint,
  .delivery {
    font-size: 12px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .delivery {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
  }
  .delivery.delivered {
    color: var(--diff-add);
  }
  .delivery.queued {
    color: var(--accent);
  }
  .spacer {
    flex: 1;
  }
  :global(.phone) .bar {
    padding: 8px 12px calc(8px + env(safe-area-inset-bottom));
  }
  :global(.phone) .row {
    flex-wrap: wrap;
  }
  :global(.phone) .hint {
    display: none;
  }
  :global(.phone) .delivery {
    flex: 1 1 100%;
    order: -1;
  }
  :global(.phone) .row .btn,
  :global(.phone) .pending {
    min-height: 44px;
  }
  :global(.phone) .row .btn {
    flex: 1 1 0;
    justify-content: center;
  }
  :global(.phone) .spacer {
    display: none;
  }
  :global(.phone) textarea {
    font-size: 15px;
  }
</style>
