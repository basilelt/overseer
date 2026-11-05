<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import SendHorizontal from '@lucide/svelte/icons/send-horizontal';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import { untrack } from 'svelte';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { enterAction } from '../../chat/composer';
  import { parsePicker, parsePrompt, PICKER_PROBES_MS, type PromptCard, type PromptOption } from '../../chat/prompt';

  let { lineId, agent, waiting, probe = 0, onterminal }: { lineId: string; agent: string; waiting: boolean; probe?: number; onterminal: () => void } = $props();

  const SETTLE_MS = 450;

  let root = $state<HTMLDivElement>();
  let card = $state<PromptCard | null>(null);
  let excerpt = $state('');
  let loading = $state(false);
  let sending = $state(false);
  let hidden = $state(false);
  let answer = $state('');
  let picker = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const multi = $derived(!!card?.submit);
  const open = $derived((waiting || picker) && !hidden);

  async function refresh(settle = false) {
    loading = true;
    try {
      const s = await app.api.screen(lineId);
      card = waiting ? parsePrompt(s.text, agent) : parsePicker(s.text, agent);
      excerpt = card ? '' : s.text.split('\n').filter((l) => l.trim()).slice(-8).join('\n');
      if (settle && !card) hidden = true;
      if (!card) picker = false;
    } catch {
      card = null;
      excerpt = '';
    } finally {
      loading = false;
    }
  }

  function later() {
    clearTimeout(timer);
    timer = setTimeout(() => void refresh(true), SETTLE_MS);
  }

  $effect(() => {
    if (!waiting) {
      hidden = false;
      return;
    }
    untrack(() => {
      hidden = false;
      void refresh();
    });
  });

  $effect(() => () => clearTimeout(timer));

  $effect(() => {
    lineId;
    picker = false;
  });

  $effect(() => {
    if (!probe) return;
    const id = untrack(() => lineId);
    const kind = untrack(() => agent);
    let step = 0;
    let probeTimer: ReturnType<typeof setTimeout> | undefined;
    let live = true;
    const next = () => {
      if (step >= PICKER_PROBES_MS.length) return;
      probeTimer = setTimeout(async () => {
        try {
          const found = parsePicker((await app.api.screen(id)).text, kind);
          if (!live || id !== lineId) return;
          if (found) {
            card = found;
            excerpt = '';
            hidden = false;
            picker = true;
            return;
          }
        } catch {
          return;
        }
        next();
      }, PICKER_PROBES_MS[step++]);
    };
    untrack(next);
    return () => {
      live = false;
      clearTimeout(probeTimer);
    };
  });

  async function press(keys: string[], text?: string) {
    if (text !== undefined) await app.api.keys(lineId, keys, text);
    else if (keys.length) await app.api.keys(lineId, keys);
  }

  async function run(fn: () => Promise<void>) {
    sending = true;
    try {
      await fn();
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      sending = false;
    }
  }

  function choose(o: PromptOption) {
    return run(async () => {
      await press(o.keys);
      if (o.chat) {
        hidden = true;
        root?.closest('.chat')?.querySelector<HTMLTextAreaElement>('.composer textarea')?.focus();
        return;
      }
      later();
    });
  }

  function type(o: PromptOption) {
    const text = answer.trim();
    if (!text) return;
    return run(async () => {
      await press(o.keys, text);
      if (!multi) await press(['enter']);
      answer = '';
      later();
    });
  }

  function onkeydown(e: KeyboardEvent, o: PromptOption) {
    if (enterAction(e, document.documentElement.classList.contains('phone')) !== 'send') return;
    e.preventDefault();
    void type(o);
  }

  function cancel() {
    return run(async () => {
      await press(['esc']);
      later();
    });
  }
</script>

{#if open}
  <div class="card" role="group" aria-label={t('chat.prompt.title')} bind:this={root}>
    <div class="top">
      <span class="dot"></span>
      <span class="title">{t('chat.prompt.title')}</span>
      <button class="icon-btn" title={t('chat.prompt.refresh')} aria-label={t('chat.prompt.refresh')} disabled={loading} onclick={() => refresh()}><RefreshCw size={13} /></button>
    </div>
    {#if card}
      {#if card.tabs}
        <div class="tabs">
          {#each card.tabs as tab, i (i)}
            <span class="tab" class:done={tab.done} class:current={tab.current} aria-current={tab.current ? 'step' : undefined}>
              {#if tab.done}<Check size={11} />{/if}{tab.label}
            </span>
          {/each}
        </div>
      {/if}
      {#if card.question}<pre class="q">{card.question}</pre>{/if}
      <div class="opts" class:column={!!card.tabs || card.options.some((o) => o.description || o.input)}>
        {#each card.options as o, i (i)}
          {#if o.input}
            <div class="field">
              <textarea rows="1" bind:value={answer} placeholder={o.label.replace(/^\d+\.\s*/, '')} disabled={sending} onkeydown={(e) => onkeydown(e, o)}></textarea>
              <button class="btn primary" title={t('chat.send')} aria-label={t('chat.send')} disabled={sending || !answer.trim()} onclick={() => type(o)}><SendHorizontal size={14} /></button>
            </div>
          {:else if o.chat}
            <button class="btn ghost talk" disabled={sending} onclick={() => choose(o)}>{o.label}</button>
          {:else}
            <button class="btn opt" class:primary={i === 0 && !multi} class:checked={o.checked} role={multi ? 'checkbox' : undefined} aria-checked={multi ? !!o.checked : undefined} disabled={sending} onclick={() => choose(o)}>
              {#if multi}<span class="box">{#if o.checked}<Check size={12} />{/if}</span>{/if}
              <span class="txt">
                <span>{o.label}</span>
                {#if o.description}<span class="desc">{o.description}</span>{/if}
              </span>
            </button>
          {/if}
        {/each}
      </div>
      {#if card.submit || card.cancel}
        <div class="actions">
          {#if card.submit}<button class="btn primary" disabled={sending} onclick={() => choose(card!.submit!)}>{card.submit.label}</button>{/if}
          {#if card.cancel}<button class="btn ghost" disabled={sending} onclick={cancel}>{t('common.cancel')}</button>{/if}
        </div>
      {/if}
    {:else if excerpt}
      <pre class="q">{excerpt}</pre>
    {:else if !loading}
      <p class="faint">{t('chat.prompt.unknown')}</p>
    {/if}
    <button class="btn ghost term" onclick={onterminal}><SquareTerminal size={13} />{t('chat.openTerminal')}</button>
  </div>
{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0 12px 8px;
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--st-input) 45%, var(--border));
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow);
    max-height: 60vh;
    overflow-y: auto;
  }
  .card > * {
    flex-shrink: 0;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--st-input);
  }
  .title {
    flex: 1;
    font-weight: 600;
    font-size: 13px;
  }
  .q {
    user-select: text;
    -webkit-user-select: text;
    margin: 0;
    max-height: 30vh;
    overflow: auto;
    font: 12px/1.45 var(--mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--text-2);
  }
  .opts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .opts .btn {
    min-height: 34px;
    max-width: 100%;
    text-align: left;
    white-space: normal;
  }
  .opts.column {
    flex-direction: column;
    flex-wrap: nowrap;
  }
  .opts.column > .btn {
    width: 100%;
  }
  .opt {
    height: auto;
    padding-top: 6px;
    padding-bottom: 6px;
    align-items: flex-start;
    gap: 8px;
  }
  .txt {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .desc {
    font-size: 11px;
    font-weight: 400;
    opacity: 0.75;
  }
  .box {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    margin-top: 1px;
    border: 1px solid var(--border-strong);
    border-radius: 3px;
  }
  .checked .box {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 11px;
    color: var(--text-2);
  }
  .tab.done {
    color: var(--text);
  }
  .tab.current {
    border-color: var(--st-input);
    background: color-mix(in srgb, var(--st-input) 15%, transparent);
    color: var(--text);
    font-weight: 600;
  }
  .field {
    width: 100%;
    display: flex;
    align-items: flex-end;
    gap: 6px;
  }
  .field textarea {
    flex: 1;
    min-width: 0;
    min-height: 34px;
    max-height: 30vh;
    padding: 7px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    resize: vertical;
    field-sizing: content;
  }
  .field textarea:focus {
    outline: none;
    border-color: var(--accent);
  }
  .field .btn {
    flex: none;
    min-height: 34px;
  }
  .talk {
    justify-content: flex-start;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .term {
    align-self: flex-start;
    gap: 6px;
  }
  p {
    margin: 0;
    font-size: 12px;
  }
  :global(.phone) .opts .btn,
  :global(.phone) .field textarea,
  :global(.phone) .actions .btn {
    min-height: 44px;
  }
  :global(.phone) .opts:not(.column) > .btn {
    flex: 1 1 100%;
  }
  :global(.phone) .opts .opt {
    padding-top: 8px;
    padding-bottom: 8px;
  }
</style>
