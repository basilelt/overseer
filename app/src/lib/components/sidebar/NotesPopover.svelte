<script lang="ts">
  import { dismiss } from '../../dismiss';
  import { isNotesTrigger } from '../../notes-trigger';
  import StickyNote from '@lucide/svelte/icons/sticky-note';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { isPhone } from '../../mobile/ui.svelte';
  import { addNote, editNote, notesOf, removeNote } from '../../notes';
  import { closeNotes, notesUi } from '../../notes.svelte';
  import { age } from '../../time';
  import { onDestroy, untrack } from 'svelte';
  import { isSubmitChord } from '../../chord';

  let box: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });
  let draft = $state('');
  let edits = $state<Record<string, string>>({});
  const lineId = $derived(notesUi.lineId!);
  const notes = $derived(notesOf(app.data, lineId));
  const title = $derived(app.items.find((i) => i.lineId === lineId)?.title ?? '');
  const phone = $derived(isPhone());

  $effect(() => {
    if (!box) return;
    const r = box.getBoundingClientRect();
    pos = { left: Math.max(4, Math.min(notesUi.x, innerWidth - r.width - 8)), top: Math.max(4, Math.min(notesUi.y, innerHeight - r.height - 8)) };
  });

  let shown = '';

  $effect(() => {
    const id = lineId;
    untrack(() => flush(shown));
    shown = id;
    box?.querySelector<HTMLTextAreaElement>('textarea.new')?.focus();
  });

  onDestroy(() => {
    flush(shown);
    closeNotes();
  });

  function flush(line: string) {
    const pending = Object.entries(edits);
    const text = draft;
    edits = {};
    draft = '';
    for (const [id, t] of pending) void saveText(line, id, t);
    if (line && text.trim()) void addNote(app, line, text);
  }

  async function saveText(line: string, id: string, text: string) {
    if (!(await editNote(app, line, id, text)) && line === shown) edits = { ...edits, [id]: text };
  }

  async function add() {
    const text = draft;
    if (!text.trim()) return;
    draft = '';
    if (!(await addNote(app, shown, text)) && !draft) draft = text;
  }

  function save(id: string) {
    const text = edits[id];
    if (text === undefined) return;
    const { [id]: _, ...rest } = edits;
    edits = rest;
    void saveText(shown, id, text);
  }

  const rowsFor = (text: string) => Math.min(8, text.split('\n').reduce((n, l) => n + Math.max(1, Math.ceil(l.length / 38)), 0));

  function onKey(e: KeyboardEvent, run: () => void) {
    if (isSubmitChord(e)) {
      e.preventDefault();
      run();
    }
  }

  const trigger = (el: Element | null) => isNotesTrigger(el, lineId);
</script>

{#if phone}<div class="scrim" role="presentation"></div>{/if}
<div class="notes" role="dialog" aria-label={t('notes.title')} bind:this={box} use:dismiss={{ onclose: closeNotes, escape: 'stop', keep: trigger }} style:left="{pos.left}px" style:top="{pos.top}px">
  <header>
    <StickyNote size={14} />
    <span class="ttl">{t('notes.title')}</span>
    <span class="who" title={title}>{title}</span>
    <button class="icon-btn" type="button" title={t('notes.close')} aria-label={t('notes.close')} onclick={closeNotes}><X size={14} /></button>
  </header>
  <div class="list">
    {#each notes as n (n.id)}
      <div class="card" data-note={n.id}>
        <textarea
          rows={rowsFor(edits[n.id] ?? n.text)}
          aria-label={t('notes.edit')}
          value={edits[n.id] ?? n.text}
          oninput={(e) => (edits = { ...edits, [n.id]: e.currentTarget.value })}
          onblur={() => save(n.id)}
          onkeydown={(e) => onKey(e, () => e.currentTarget.blur())}
          disabled={n.id.startsWith('pending-')}
        ></textarea>
        <div class="meta">
          <span class="when" title={n.updatedAt}>{age(n.updatedAt)}</span>
          <button class="icon-btn del" type="button" title={t('notes.delete')} aria-label={t('notes.delete')} disabled={n.id.startsWith('pending-')} onclick={() => removeNote(app, lineId, n.id)}><Trash2 size={13} /></button>
        </div>
      </div>
    {:else}
      <p class="empty">{t('notes.empty')}</p>
    {/each}
  </div>
  <div class="add">
    <textarea class="new" rows="2" placeholder={t(phone ? 'notes.placeholderPhone' : 'notes.placeholder')} aria-label={t('notes.add')} bind:value={draft} onkeydown={(e) => onKey(e, add)}></textarea>
    <button class="btn" type="button" disabled={!draft.trim()} onclick={add}>{t('notes.addButton')}</button>
  </div>
</div>

<style>
  .notes {
    position: fixed;
    z-index: var(--z-popover);
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 300px;
    max-height: min(480px, calc(100dvh - 16px));
    padding: 10px;
    background: var(--surface);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow);
  }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--note-ink);
  }
  .ttl {
    flex: none;
    font-weight: 700;
    color: var(--text);
  }
  .who {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--text-2);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    min-height: 0;
  }
  .card {
    padding: 6px 8px 4px;
    border: 1px solid var(--note-border);
    border-radius: var(--radius);
    background: var(--note-bg);
  }
  textarea {
    display: block;
    width: 100%;
    max-height: 200px;
    resize: none;
    border: 0;
    padding: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    line-height: 1.35;
    outline: none;
  }
  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-2);
  }
  .del {
    width: 22px;
    height: 22px;
  }
  .empty {
    margin: 4px 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .add {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .new {
    padding: 6px 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--bg);
  }
  .new:focus {
    border-color: var(--accent);
  }
  .btn {
    align-self: flex-end;
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: var(--z-popover-scrim);
    background: var(--overlay);
  }
  :global(.phone) .notes {
    left: 0 !important;
    right: 0;
    top: auto !important;
    bottom: 0;
    width: auto;
    max-height: 80dvh;
    padding: 12px 12px calc(12px + env(safe-area-inset-bottom));
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
  }
  :global(:root.phone) .card textarea {
    min-height: 0;
  }
</style>
