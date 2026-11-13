<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import Circle from '@lucide/svelte/icons/circle';
  import CircleDot from '@lucide/svelte/icons/circle-dot';
  import Copy from '@lucide/svelte/icons/copy';
  import ImageIcon from '@lucide/svelte/icons/image';
  import { hhmm } from '../../chat/time';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { COPIED_FLASH_MS } from '../../timing';
  import type { ChatEntry } from '../../types';
  import Markdown from './Markdown.svelte';
  import ToolStep from './ToolStep.svelte';
  import SubagentCard from './SubagentCard.svelte';
  import ChatImage from './ChatImage.svelte';

  let { entry, showModel = false, lineId }: { entry: ChatEntry; showModel?: boolean; lineId?: string } = $props();

  const time = $derived(entry.at ? hhmm(entry.at) : '');
  let copied = $state(false);

  async function copy() {
    try {
      await app.client?.copyText(entry.text ?? '');
      copied = true;
      setTimeout(() => (copied = false), COPIED_FLASH_MS);
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }
</script>

{#snippet meta(label: string)}
  <span class="meta">
    {#if label}<span class="time faint">{label}</span>{/if}
    {#if entry.text}
      <button class="copy" title={t(copied ? 'chat.copied' : 'chat.copyMessage')} aria-label={t('chat.copyMessage')} onclick={copy}>
        {#if copied}<Check size={12} />{:else}<Copy size={12} />{/if}
      </button>
    {/if}
  </span>
{/snippet}

{#if entry.kind === 'user'}
  <div class="row user">
    <div class="bubble">
      {#if entry.images && lineId && entry.imagesAt !== undefined}
        <div class="imgs">
          {#each { length: entry.images } as _, i (i)}<ChatImage {lineId} at={entry.imagesAt} {i} />{/each}
        </div>
      {:else if entry.images}<span class="img faint"><ImageIcon size={12} />{t('chat.images', { n: entry.images })}</span>{/if}
      <div class="utext">{entry.text ?? ''}</div>
    </div>
    {@render meta(time)}
  </div>
{:else if entry.kind === 'assistant'}
  <div class="row assistant">
    <Markdown text={entry.text ?? ''} />
    {@render meta(`${time}${showModel && entry.model ? `${time ? ' · ' : ''}${entry.model}` : ''}`)}
  </div>
{:else if entry.kind === 'tool' && entry.tool}
  <div class="row tool">{#if entry.tool.agent}<SubagentCard id={entry.id} tool={entry.tool} />{:else}<ToolStep tool={entry.tool} />{/if}</div>
{:else if entry.kind === 'todos' && entry.todos}
  <div class="row todos">
    {#each entry.todos as td, i (i)}
      <div class="todo {td.state}">
        <span class="ti">
          {#if td.state === 'completed'}<Check size={13} />{:else if td.state === 'in_progress'}<CircleDot size={13} />{:else}<Circle size={13} />{/if}
        </span>
        <span>{td.text}</span>
      </div>
    {/each}
  </div>
{:else if entry.kind === 'notice'}
  <div class="row notice faint">{entry.text ?? ''}</div>
{/if}

<style>
  .row {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .user {
    align-items: flex-end;
  }
  .bubble {
    max-width: min(85%, 720px);
    padding: 8px 12px;
    border-radius: 14px 14px 4px 14px;
    background: var(--accent-soft);
    color: var(--text);
  }
  .utext {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    line-height: 1.5;
  }
  .imgs {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
  }
  .imgs:not(:last-child) {
    margin-bottom: 6px;
  }
  .img {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
  }
  .time {
    font-size: 10.5px;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 20px;
    user-select: none;
    -webkit-user-select: none;
  }
  .copy {
    display: grid;
    place-items: center;
    width: 22px;
    height: 20px;
    border-radius: var(--radius);
    color: var(--text-3);
    opacity: 0;
  }
  .row:hover .copy,
  .copy:focus-visible,
  :global(.phone) .copy {
    opacity: 1;
  }
  .copy:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .assistant {
    color: var(--text);
  }
  .todos {
    gap: 3px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    font-size: 12.5px;
  }
  .todo {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    color: var(--text);
  }
  .ti {
    display: grid;
    padding-top: 2px;
    color: var(--text-3);
  }
  .todo.completed {
    color: var(--text-3);
    text-decoration: line-through;
  }
  .todo.completed .ti {
    color: var(--st-input);
  }
  .todo.in_progress .ti {
    color: var(--st-working);
  }
  .notice {
    align-items: center;
    font-size: 11.5px;
    text-align: center;
  }
</style>
