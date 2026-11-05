<script lang="ts">
  import Bot from '@lucide/svelte/icons/bot';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { getContext, tick, untrack } from 'svelte';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { agentName, elapsedMs, formatElapsed, formatTokens } from '../../chat/agents';
  import { clock, tickWhile } from '../../chat/clock.svelte';
  import { applyEarlier, applyTail, applyUpdate, EMPTY_CHAT, type ChatState } from '../../chat/store';
  import type { ChatTool } from '../../types';
  import Entry from './Entry.svelte';

  let { id, tool }: { id: string; tool: ChatTool } = $props();

  const lineOf = getContext<() => string>('chat-line');
  const a = $derived(tool.agent!);
  const running = $derived(a.state === 'running');
  let open = $state(false);
  let sub = $state.raw<ChatState>(EMPTY_CHAT);
  let failed = $state(false);
  let card = $state<HTMLDivElement>();
  let fetching = false;
  let again = false;

  const elapsed = $derived(formatElapsed(elapsedMs(a, running ? clock.now : Date.now())));
  const canOpen = $derived(!!a.agentId || !!tool.output);

  $effect(() => {
    if (running) return tickWhile();
  });

  $effect(() => {
    const el = card;
    el?.addEventListener('reveal', reveal);
    return () => el?.removeEventListener('reveal', reveal);
  });

  $effect(() => {
    tool;
    if (open) untrack(() => void load());
  });

  async function load() {
    const agentId = a.agentId;
    if (!agentId || !lineOf) return;
    if (fetching) return void (again = true);
    fetching = true;
    try {
      do {
        again = false;
        sub = sub.loaded ? applyUpdate(sub, await app.api.chat(lineOf(), { after: sub.cursor }, agentId)) : applyTail(EMPTY_CHAT, await app.api.chat(lineOf(), { limit: 200 }, agentId));
        failed = false;
      } while (again);
    } catch {
      failed = !sub.loaded;
    } finally {
      fetching = false;
    }
  }

  async function reveal() {
    open = true;
    await tick();
    await load();
    await tick();
    card?.scrollIntoView({ block: 'start' });
  }

  async function earlier() {
    const first = sub.entries[0]?.seq;
    if (first === undefined || !a.agentId || !lineOf) return;
    try {
      sub = applyEarlier(sub, await app.api.chat(lineOf(), { before: first, limit: 200 }, a.agentId));
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }
</script>

<div class="agent {a.state}" class:open id="agent-{id}" bind:this={card}>
  <button class="head" disabled={!canOpen} aria-expanded={open} aria-label={t('chat.agent.open', { name: agentName(a) })} onclick={() => (open = !open)}>
    <span class="st {a.state}" title={t(`chat.state.${a.state}`)}></span>
    <span class="ic"><Bot size={13} /></span>
    <span class="type">{agentName(a)}</span>
    <span class="sum">{a.description || tool.label}</span>
    {#if canOpen}<span class="chev"><ChevronRight size={13} /></span>{/if}
  </button>
  <div class="meta faint">
    <span>{running ? elapsed : t(`chat.state.${a.state}`) + ' · ' + elapsed}</span>
    {#if a.tokens}<span>· ↓ {t('chat.agent.tokens', { n: formatTokens(a.tokens) })}</span>{/if}
    {#if a.toolCount}<span>· {t('chat.agent.tools', { n: a.toolCount })}</span>{/if}
    {#if a.background}<span class="bg">· {t('chat.agent.background')}</span>{/if}
    {#if running && a.lastActivity}<span class="act">⎿ {a.lastActivity}</span>{/if}
  </div>
  {#if open}
    <div class="body">
      {#if !a.agentId && tool.output}
        <pre class="out">{tool.output}</pre>
      {:else if failed}
        <p class="note faint">{t('chat.agent.unavailable')}</p>
      {:else if !sub.loaded}
        <p class="note faint">{t('chat.loading')}</p>
      {:else}
        {#if sub.more}<button class="earlier" onclick={earlier}>{t('chat.loadEarlier')}</button>{/if}
        {#each sub.entries as e (e.id)}
          <div class="item"><Entry entry={e} /></div>
        {:else}
          <p class="note faint">{t('chat.agent.empty')}</p>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .agent {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    min-width: 0;
    scroll-margin: 48px;
  }
  .agent.running {
    border-color: color-mix(in srgb, var(--st-working) 45%, var(--border));
  }
  .head {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    min-height: 30px;
    padding: 4px 8px 0;
    border: 0;
    background: none;
    color: var(--text-2);
    font: 12.5px var(--font);
    text-align: left;
    cursor: pointer;
  }
  .head:disabled {
    cursor: default;
  }
  .st {
    flex-shrink: 0;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--st-done);
  }
  .st.running {
    background: var(--st-working);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .st.failed {
    background: var(--st-failed);
  }
  .st.stopped {
    background: var(--st-held);
  }
  .ic {
    display: grid;
    color: var(--text-3);
  }
  .type {
    flex-shrink: 0;
    font-weight: 600;
    color: var(--text);
  }
  .sum {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .chev {
    display: grid;
    margin-left: auto;
    color: var(--text-3);
    transition: transform var(--ease);
  }
  .open .chev {
    transform: rotate(90deg);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0 5px;
    padding: 0 8px 5px 37px;
    font-size: 11px;
    min-width: 0;
  }
  .act {
    flex-basis: 100%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    color: var(--text-2);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0 8px 8px 12px;
    padding: 8px 0 0 10px;
    border-top: 1px solid var(--border);
    border-left: 2px solid var(--border);
    min-width: 0;
  }
  .item {
    min-width: 0;
  }
  .note {
    margin: 4px 0;
    font-size: 12px;
  }
  .earlier {
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 11.5px;
    cursor: pointer;
  }
  pre {
    margin: 0;
    padding: 6px 8px;
    border-radius: var(--radius);
    background: var(--bg-subtle);
    font: 11.5px/1.45 var(--mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 320px;
    overflow: auto;
  }
</style>
