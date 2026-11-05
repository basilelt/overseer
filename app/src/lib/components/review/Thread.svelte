<script lang="ts">
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import { untrack } from 'svelte';
  import Pencil from '@lucide/svelte/icons/pencil';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Trash from '@lucide/svelte/icons/trash';
  import { agentName } from '../../agents';
  import { t, type Key } from '../../i18n.svelte';
  import type { ReviewStore } from '../../review/store.svelte';
  import { CHIP_TONE, chips, rangeLabel, threadSuggestion } from '../../review/threads';
  import type { Thread } from '../../review/types';
  import { app } from '../../state.svelte';
  import { age } from '../../time';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import Body from './Body.svelte';
  import CommentBox from './CommentBox.svelte';

  let { store, thread, showCode = false }: { store: ReviewStore; thread: Thread; showCode?: boolean } = $props();

  const agent = $derived.by(() => {
    const id = thread.lineId ?? store.target.lineId;
    return app.data.lines.find((l) => l.line.id === id)?.line.agent ?? 'claude';
  });
  const resolved = $derived(thread.status === 'resolved');
  let open = $state(untrack(() => thread.status !== 'resolved'));
  let replying = $state(false);
  let reply = $state('');
  let editing = $state<string | null>(null);
  let editBody = $state('');
  let applying = $state(false);
  const suggestion = $derived(threadSuggestion(thread));
  const canApply = $derived(!!suggestion && !thread.outdated && !thread.changed && thread.side === 'new' && !resolved);

  async function sendReply() {
    if (await store.reply(thread.id, reply)) {
      reply = '';
      replying = false;
    }
  }

  async function saveEdit(id: string) {
    await store.edit(id, editBody);
    editing = null;
  }

  async function apply() {
    applying = true;
    await store.apply(thread);
    applying = false;
  }
</script>

<div class="thread" class:resolved data-thread={thread.id}>
  <button class="thead" onclick={() => (open = !open)} aria-expanded={open}>
    <span class="where mono">{rangeLabel(thread)}</span>
    {#each chips(thread) as c (c)}
      <span class="chip {CHIP_TONE[c]}">{#if c === 'queued'}<span class="pulse"></span>{/if}{t(`review.chip.${c}` as Key)}</span>
    {/each}
    <span class="spacer"></span>
    {#if !open}<span class="faint count">{thread.comments.length}</span>{/if}
  </button>
  {#if open}
    {#if showCode || thread.outdated || thread.changed}
      <div class="orig">
        <div class="olabel faint">{t('review.originalCode')} · L{thread.originalLine}{thread.originalEndLine !== thread.originalLine ? `–${thread.originalEndLine}` : ''}</div>
        <pre class="mono">{thread.code}</pre>
      </div>
    {/if}
    {#each thread.comments as c (c.id)}
      <div class="comment" class:pending={c.pending}>
        <div class="who">
          {#if c.author === 'agent'}
            <span class="avatar agent"><AgentGlyph {agent} size={12} /></span>
            <span class="name">{agentName(app.agents, agent)}</span>
          {:else}
            <span class="avatar">{t('review.you').slice(0, 1)}</span>
            <span class="name">{t('review.you')}</span>
          {/if}
          <span class="faint">{age(c.createdAt)}</span>
          {#if c.pending}<span class="chip warn">{t('review.chip.pending')}</span>{/if}
          <span class="spacer"></span>
          {#if c.pending && c.author === 'user' && editing !== c.id}
            <button class="icon-btn sm" title={t('review.edit')} aria-label={t('review.edit')} onclick={() => ((editing = c.id), (editBody = c.body))}><Pencil size={12} /></button>
            <button class="icon-btn sm" title={t('review.discard')} aria-label={t('review.discard')} onclick={() => store.remove(c.id, thread.id)}><Trash size={12} /></button>
          {/if}
        </div>
        {#if editing === c.id}
          <CommentBox bind:value={editBody} label={t('review.save')} lines={thread.code.split('\n')} onsubmit={() => saveEdit(c.id)} oncancel={() => (editing = null)} />
        {:else}
          <Body body={c.body} code={thread.code} onapply={canApply && c.author === 'user' ? apply : undefined} {applying} />
        {/if}
      </div>
    {/each}
    <div class="foot">
      {#if replying}
        <CommentBox bind:value={reply} label={t('review.reply')} lines={thread.code.split('\n')} rows={2} onsubmit={sendReply} oncancel={() => (replying = false)} />
      {:else}
        <button class="replyfake" onclick={() => (replying = true)}>{t('review.replyPlaceholder')}</button>
        {#if resolved}
          <button class="btn small" onclick={() => store.setStatus(thread, 'reopen')}><RotateCcw size={12} />{t('review.reopen')}</button>
        {:else}
          <button class="btn small" onclick={() => store.setStatus(thread, 'resolve')}><CircleCheck size={12} />{t('review.resolve')}</button>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .thread {
    margin: 6px 12px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    font-family: var(--font);
    white-space: normal;
    max-width: 860px;
    overflow: hidden;
  }
  .thead {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 10px;
    text-align: left;
    font-size: 11.5px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .thread.resolved .thead {
    border-bottom-color: transparent;
  }
  .where {
    color: var(--text-2);
    font-size: 11px;
  }
  .spacer {
    flex: 1;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 18px;
    padding: 0 7px;
    border-radius: 9px;
    font-size: 10.5px;
    font-weight: 500;
    border: 1px solid var(--border);
    color: var(--text-2);
    white-space: nowrap;
  }
  .chip.accent {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 30%, transparent);
    background: var(--accent-soft);
  }
  .chip.warn {
    color: #b45309;
    border-color: color-mix(in srgb, #d97706 35%, transparent);
    background: color-mix(in srgb, #f59e0b 10%, transparent);
  }
  :global([data-theme='dark']) .chip.warn {
    color: #fbbf24;
  }
  .chip.ok {
    color: var(--diff-add);
    border-color: color-mix(in srgb, var(--diff-add) 30%, transparent);
    background: var(--diff-add-bg);
  }
  .chip.muted {
    color: var(--text-3);
  }
  .pulse {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }
  .count {
    font-size: 11px;
  }
  .orig {
    padding: 8px 12px 0;
  }
  .olabel {
    font-size: 11px;
    margin-bottom: 4px;
  }
  .orig pre {
    margin: 0;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    font-size: 11.5px;
    overflow-x: auto;
    color: var(--text-2);
  }
  .comment {
    padding: 10px 12px 8px;
  }
  .comment + .comment {
    border-top: 1px solid var(--border);
  }
  .comment.pending {
    background: color-mix(in srgb, #f59e0b 4%, transparent);
  }
  .who {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-bottom: 4px;
    font-size: 12px;
  }
  .avatar {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--surface-active);
    color: var(--text-2);
    font-size: 10.5px;
    font-weight: 600;
  }
  .avatar.agent {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .name {
    font-weight: 600;
  }
  .sm {
    width: 22px;
    height: 22px;
  }
  .foot {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 8px 12px 10px;
    border-top: 1px solid var(--border);
  }
  .foot :global(.box) {
    flex: 1;
  }
  .replyfake {
    flex: 1;
    height: 28px;
    text-align: left;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text-3);
    background: var(--surface);
    font-size: 12px;
    cursor: text;
  }
  .small {
    height: 28px;
    font-size: 12px;
  }
</style>
