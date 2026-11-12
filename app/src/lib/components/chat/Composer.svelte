<script lang="ts" module>
  import { Drafts } from '../../drafts';

  const drafts = new Drafts(KEY_DRAFTS_CHAT);
</script>

<script lang="ts">
  import { atEdge, recall, type Recall } from '../../chat/history';
  import { dropZone } from '../../drop';
  import Mic from '@lucide/svelte/icons/mic';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import SendHorizontal from '@lucide/svelte/icons/send-horizontal';
  import X from '@lucide/svelte/icons/x';
  import { onMount } from 'svelte';
  import { isTauri } from '../../client';
  import { dictationHintKey, dictationMode, startDictation } from '../../dictation';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import type { CustomCommand, LineLocal } from '../../types';
  import { clipboardFiles, fileInfos, insertText, planDrop, planPaste, refusalMessage, uploadedPath, type Refusal } from '../../uploads';
  import { savePasteAll, uploadList } from '../../upload-pipeline';
  import { requirePath, savePasted } from '../../task-attachments';
  import { applySlash, commandsFor, enterAction, lineCount, slashMatches, slashQuery, type SendMode, type SlashCommand } from '../../chat/composer';
  import { ChatSender } from '../../chat/sender.svelte';
  import { clearComposerHeight, reportComposerHeight } from '../../chat/composer-height';
  import { platformName } from '../../platform';
  import { KEY_DRAFTS_CHAT } from '../../storage';

  let { lineId, agent, local, phone, history = [], onsent }: { lineId: string; agent: string; local: LineLocal | undefined; phone: boolean; history?: string[]; onsent?: (text: string) => void } = $props();
  let back: Recall = { index: -1, draft: '' };

  let box = $state<HTMLTextAreaElement>();
  let root = $state<HTMLDivElement>();
  let picker = $state<HTMLInputElement>();
  let text = $state('');
  const sender = new ChatSender((req) => app.api.broadcast(req));
  let dropping = $state(false);
  let caret = $state(0);
  let custom = $state<CustomCommand[] | null>(null);
  let pick = $state(0);
  let dismissed = $state(false);

  const remote = $derived(app.conn.target.kind !== 'local');
  const query = $derived(dismissed ? null : slashQuery(text, caret));
  const matches = $derived<SlashCommand[]>(query === null ? [] : slashMatches(query, commandsFor(agent, custom ?? [])));
  const message = (e: unknown) => String((e as Error)?.message ?? e);
  const os = platformName();
  const dictation = $derived(dictationMode(app.os, os, isTauri(), phone));

  async function dictate() {
    box?.focus();
    if (dictation !== 'native') return app.notify(t(dictationHintKey(os)));
    try {
      await startDictation();
    } catch (e) {
      app.notify(t('dictation.failed', { reason: message(e) }), 'error');
    }
  }

  $effect(() => {
    const id = lineId;
    text = drafts.get(id) ?? '';
    back = { index: -1, draft: '' };
    custom = null;
    sender.reset(id);
  });

  $effect(() => {
    drafts.set(lineId, text);
  });

  $effect(() => {
    if (query !== null && custom === null) {
      custom = [];
      app.api
        .commands(lineId)
        .then((r) => (custom = r.commands ?? []))
        .catch(() => {});
    }
  });

  $effect(() => {
    matches.length;
    pick = 0;
  });

  $effect(() => {
    text;
    autosize();
  });

  let sized = { length: 0, lines: 0 };
  let sizeFrame = 0;
  function autosize() {
    cancelAnimationFrame(sizeFrame);
    sizeFrame = requestAnimationFrame(() => {
      const el = box;
      if (!el) return;
      const now = { length: text.length, lines: lineCount(text) };
      const shrink = now.length < sized.length || now.lines < sized.lines;
      sized = now;
      if (shrink) el.style.height = 'auto';
      else if (el.scrollHeight <= el.clientHeight) return;
      const h = `${Math.min(el.scrollHeight, Math.round(window.innerHeight * 0.4))}px`;
      if (el.style.height !== h) el.style.height = h;
    });
  }

  onMount(() => () => cancelAnimationFrame(sizeFrame));

  $effect(() => {
    const el = root;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const key = Symbol('composer');
    const ro = new ResizeObserver(() => reportComposerHeight(key, el.offsetHeight));
    ro.observe(el);
    return () => {
      ro.disconnect();
      clearComposerHeight(key);
    };
  });

  $effect(() => {
    const el = box;
    if (!el || typeof ResizeObserver === 'undefined') return;
    let width = el.clientWidth;
    const ro = new ResizeObserver(() => {
      if (el.clientWidth === width) return;
      width = el.clientWidth;
      sized = { length: Infinity, lines: Infinity };
      autosize();
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  function insert(s: string) {
    const el = box;
    const at = el ? el.selectionStart : text.length;
    const end = el ? el.selectionEnd : text.length;
    text = text.slice(0, at) + s + text.slice(end);
    queueMicrotask(() => {
      if (!el) return;
      el.focus();
      el.selectionStart = el.selectionEnd = at + s.length;
      caret = el.selectionStart;
    });
  }

  async function send(mode: SendMode) {
    const id = lineId;
    const out = await sender.send(text, id, local, mode);
    if (out.kind === 'failed') return app.notify(t('mobile.compose.failed', { reason: out.reason }), 'error');
    if (out.kind !== 'sent' && out.kind !== 'queued') return;
    if (out.kind === 'sent') onsent?.(out.text);
    if (id !== lineId) return drafts.delete(id);
    if (text !== out.text) return;
    text = '';
    back = { index: -1, draft: '' };
    drafts.delete(id);
  }

  function choose(c: SlashCommand) {
    text = applySlash(text, c);
    dismissed = true;
    queueMicrotask(() => {
      box?.focus();
      if (box) caret = box.selectionStart = box.selectionEnd = text.length;
    });
  }

  function onkeydown(e: KeyboardEvent) {
    if (matches.length) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        pick = (pick + (e.key === 'ArrowDown' ? 1 : matches.length - 1)) % matches.length;
        return;
      }
      if (e.key === 'Tab' || (e.key === 'Enter' && !e.shiftKey && !phone)) {
        e.preventDefault();
        return choose(matches[pick]);
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        dismissed = true;
        return;
      }
    }
    if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && !e.shiftKey && !e.altKey && !e.metaKey && !e.ctrlKey && box) {
      const dir = e.key === 'ArrowUp' ? 'up' : 'down';
      const step = atEdge(text, box.selectionStart, box.selectionEnd, dir) ? recall(history, back, text, dir) : null;
      if (step) {
        e.preventDefault();
        back = step.state;
        text = step.text;
        queueMicrotask(() => {
          if (box) caret = box.selectionStart = box.selectionEnd = dir === 'up' ? 0 : text.length;
        });
        return;
      }
    }
    if (enterAction(e, phone) === 'send') {
      e.preventDefault();
      void send('idle');
    }
  }

  function oninput() {
    caret = box?.selectionStart ?? text.length;
    if (!text.startsWith('/')) dismissed = false;
  }

  const refuse = (r: Refusal) => app.notify(refusalMessage(r), 'error');

  async function uploadAll(paths: string[]): Promise<string[]> {
    const done = await uploadList((p, onProgress) => app.requireClient().uploadFile(p, onProgress), paths, {
      progress: (name, fraction) => app.progress(t('upload.progress', { name }), fraction),
      validate: (r) => void requirePath(r?.path),
      done: (name, r, large) => {
        if (large) app.notice({ kind: 'upload', title: t('upload.done', { name, path: uploadedPath(r) }) });
      },
      fail: (name, e) => app.notify(t('upload.failed', { name, error: message(e) }), 'error', undefined, 'upload'),
    });
    return done.map(uploadedPath);
  }

  async function addFiles(files: File[]) {
    if (!files.length) return;
    const plan = planPaste({ text: '', files: fileInfos(files) }, remote);
    if (plan.kind === 'reject') return refuse(plan);
    if (plan.kind !== 'save') return;
    const saved = await savePasteAll(files, (name, bytes) => savePasted(app.requireClient(), name, bytes), (name, e) => app.notify(t('upload.pasteFailed', { name, error: message(e) }), 'error'));
    if (!saved) return;
    const paths = await uploadAll(saved);
    if (paths.length) insert(insertText(paths));
  }

  function onpaste(e: ClipboardEvent) {
    const dt = e.clipboardData;
    if (!dt) return;
    const files = clipboardFiles(dt);
    if (!files.length) return;
    e.preventDefault();
    void addFiles(files);
  }

  function picked(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    input.value = '';
    void addFiles(files);
  }

  onMount(() => {
    const offQueue = app.onBroadcast((d) => {
      if (d.target !== `line:${lineId}`) return;
      sender.queued = false;
      if (d.result !== 'sent') app.notify(t('mobile.compose.failed', { reason: d.reason ?? d.result }), 'error');
    });
    const offDrop = dropZone({
      el: () => root?.closest('.chat') ?? root,
      rank: 2,
      hover: (on) => (dropping = on),
      drop: async (paths) => {
        const plan = planDrop({ paths }, remote);
        if (plan.kind === 'reject') return refuse(plan);
        if (plan.kind === 'type') insert(insertText(plan.paths));
        else if (plan.kind === 'upload') insert(insertText(await uploadAll(plan.paths)));
      },
    });
    return () => {
      offQueue();
      offDrop();
    };
  });
</script>

<div class="composer" class:dropping bind:this={root}>
  {#if matches.length}
    <div class="slash" role="listbox" aria-label={t('chat.commands')}>
      {#each matches as c, i (c.name)}
        <button class="cmd" class:on={i === pick} role="option" aria-selected={i === pick} onpointerdown={(e) => e.preventDefault()} onclick={() => choose(c)}>
          <span class="cn">{c.name}</span>
          <span class="cd faint">{c.description}</span>
          {#if c.source !== 'builtin'}<span class="src faint">{t(c.source === 'user' ? 'chat.cmdUser' : 'chat.cmdProject')}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
  {#if sender.asking}
    <div class="ask">
      <span class="faint">{t(local?.needsInput ? 'chat.waitingChoice' : 'chat.busyChoice')}</span>
      <div class="ab">
        <button class="btn primary" disabled={sender.sending} onclick={() => send('queue')}>{t('chat.queue')}</button>
        <button class="btn" disabled={sender.sending} onclick={() => send('now')}>{t('mobile.compose.now')}</button>
        <button class="icon-btn" aria-label={t('common.cancel')} onclick={() => (sender.asking = false)}><X size={14} /></button>
      </div>
    </div>
  {:else if sender.queued}
    <div class="ask"><span class="faint">{t('mobile.compose.queued')}</span></div>
  {/if}
  <div class="row" class:shell={text.startsWith('!')}>
    <button class="icon-btn attach" title={t('chat.attach')} aria-label={t('chat.attach')} onclick={() => picker?.click()}><Paperclip size={16} /></button>
    <textarea
      bind:this={box}
      bind:value={text}
      rows="1"
      placeholder={t(phone ? 'chat.placeholderPhone' : 'chat.placeholder')}
      {onkeydown}
      {oninput}
      onclick={oninput}
      {onpaste}
      disabled={sender.sending}
      spellcheck="true"
    ></textarea>
    {#if dictation}
      <button class="icon-btn mic" title={t('dictation.start')} aria-label={t('dictation.start')} disabled={sender.sending} onpointerdown={(e) => e.preventDefault()} onclick={dictate}><Mic size={16} /></button>
    {/if}
    <button class="send" title={t('chat.send')} aria-label={t('chat.send')} disabled={sender.sending || !text.trim()} onclick={() => send('idle')}><SendHorizontal size={16} /></button>
  </div>
  <input class="picker" type="file" multiple bind:this={picker} onchange={picked} />
</div>

<style>
  .composer {
    position: relative;
    padding: 8px 12px 10px;
    border-top: 1px solid var(--border);
    background: var(--bg);
  }
  .dropping {
    outline: 1px dashed var(--accent);
    outline-offset: -4px;
  }
  .row {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    padding: 4px 4px 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
  }
  .row:focus-within {
    border-color: var(--border-strong);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .row.shell,
  .row.shell:focus-within {
    border-color: var(--st-input);
  }
  .shell textarea {
    font-family: var(--mono);
  }
  textarea {
    flex: 1;
    min-width: 0;
    min-height: 32px;
    max-height: 40vh;
    padding: 6px 2px;
    border: 0;
    outline: 0;
    resize: none;
    background: none;
    color: var(--text);
    font: 14px/1.45 var(--font);
  }
  .attach,
  .mic,
  .send {
    flex-shrink: 0;
    width: 32px;
    height: 32px;
  }
  .send {
    display: grid;
    place-items: center;
    border: 0;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--accent-fg);
    cursor: pointer;
  }
  .send:disabled {
    background: var(--surface-active);
    color: var(--text-3);
    cursor: default;
  }
  :global(.phone) .attach,
  :global(.phone) .send {
    width: 44px;
    height: 44px;
  }
  :global(.phone) textarea {
    font-size: 16px;
  }
  .ask {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 6px;
    font-size: 12px;
  }
  .ab {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .slash {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: calc(100% - 4px);
    max-height: 280px;
    overflow-y: auto;
    padding: 4px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow);
    z-index: 5;
  }
  .cmd {
    display: flex;
    align-items: baseline;
    gap: 10px;
    width: 100%;
    min-height: 32px;
    padding: 4px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .cmd.on,
  .cmd:hover {
    background: var(--surface-hover);
  }
  .cn {
    font: 12.5px var(--mono);
  }
  .cd {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }
  .src {
    font-size: 11px;
  }
  :global(.phone) .cmd {
    min-height: 44px;
  }
  .picker {
    display: none;
  }
</style>
