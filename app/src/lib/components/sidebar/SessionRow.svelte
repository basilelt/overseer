<script lang="ts">
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import StickyNote from '@lucide/svelte/icons/sticky-note';
  import { notesUi, toggleNotes } from '../../notes.svelte';
  import type { SidebarItem } from '../../session/grouping';
  import { t } from '../../i18n.svelte';
  import { age } from '../../time';
  import { isShellItem } from '../../agents';
  // `now` comes from the shared clock so rows own no timer.
  let {
    item,
    now,
    selected = false,
    multi = false,
    onselect,
    onmenu,
    onpointerdown,
    zone,
    dropPos = null,
  }: {
    item: SidebarItem;
    now: number;
    selected?: boolean;
    multi?: boolean;
    onselect: (e?: MouseEvent) => void;
    onmenu?: (e: MouseEvent) => void;
    onpointerdown?: (e: PointerEvent) => void;
    /** The workspace this row is listed in; makes it a drop target for reordering its tabs. */
    zone?: string;
    dropPos?: 'before' | 'after' | 'inside' | null;
  } = $props();

  const label = $derived(t(`session.state.${item.state}`));
  const shell = $derived(isShellItem(item));
  const notes = $derived(item.notes ?? 0);
  const notesLabel = $derived(notes ? t(notes === 1 ? 'notes.countOne' : 'notes.count', { n: notes }) : t('notes.add'));
</script>

<div class="wrap" class:noted={notes > 0} class:agent={!!item.lineId}>

<button
  class="row"
  type="button"
  data-key={item.key}
  data-drop={zone !== undefined ? 'session' : undefined}
  data-id={zone !== undefined ? item.key : undefined}
  data-zone={zone}
  class:drop-before={dropPos === 'before'}
  class:drop-after={dropPos === 'after'}
  class:multi
  class:shell aria-current={selected ? 'true' : undefined} onclick={(e) => onselect(e)} oncontextmenu={(e) => onmenu && (e.preventDefault(), onmenu(e))} {onpointerdown}>
  <span class="sr-only">{label}</span>
  {#if shell}<span class="icon" aria-hidden="true"><SquareTerminal size={12} /></span>
  {:else}<span class="dot" data-state={item.state} data-hollow={item.state === 'closed' ? '' : undefined} style:--c="var(--state-{item.state})" aria-hidden="true">{item.state === 'paused' ? '❙❙' : ''}</span>{/if}
  <span class="title">{item.title}</span>
  {#if item.status}<span class="action">{item.status}</span>{/if}
  <span class="age">{age(item.at, now)}</span>
  {#if item.lineId}<span class="nslot" aria-hidden="true"></span>{/if}
  <span class="chev" aria-hidden="true">›</span>
</button>
{#if item.lineId}
  <button
    class="note"
    type="button"
    data-notes={item.lineId}
    data-tour={notes ? 'session-notes' : undefined}
    title={notesLabel}
    aria-label={notesLabel}
    aria-expanded={notesUi.lineId === item.lineId}
    onpointerdown={(e) => e.stopPropagation()}
    onclick={(e) => toggleNotes(item.lineId!, e.currentTarget.getBoundingClientRect())}><StickyNote size={13} /></button
  >
{/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .note {
    position: absolute;
    top: 50%;
    right: 20px;
    transform: translateY(-50%);
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--text-2);
    opacity: 0;
  }
  .note:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .wrap:hover .note,
  .wrap:focus-within .note,
  .note[aria-expanded='true'] {
    opacity: 1;
  }
  .wrap.noted .note {
    opacity: 1;
    color: var(--note-ink);
  }
  .wrap.agent:not(.noted):hover .age,
  .wrap.agent:not(.noted):focus-within .age {
    visibility: hidden;
  }
  .nslot {
    display: none;
    flex: none;
    width: 14px;
  }
  .wrap.noted .nslot {
    display: block;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    margin-left: 20px;
    width: calc(100% - 20px);
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: var(--session-row-bg);
    text-align: left;
    min-width: 0;
    overflow: hidden;
  }
  .drop-before {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .drop-after {
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .row:hover {
    background: var(--surface-hover);
  }
  .row[aria-current='true'] {
    background: var(--session-row-selected);
  }
  .row.multi {
    background: color-mix(in srgb, var(--accent) 25%, var(--session-row-bg));
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .shell {
    height: 24px;
    background: none;
    color: var(--text-2);
  }
  .icon {
    flex: none;
    display: inline-grid;
    margin: 0 -2px;
    color: var(--text-2);
  }
  .dot {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
    color: var(--c);
    font-size: 7px;
    letter-spacing: -2px;
  }
  .dot[data-state='paused'] {
    width: auto;
    height: 10px;
    background: none;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: -1px;
  }
  .dot[data-hollow] {
    background: none;
    box-shadow: inset 0 0 0 1.5px var(--c);
  }
  .title {
    flex: 0 1 auto;
    min-width: 80px;
    max-width: 100%;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .action {
    flex: 1 1 0;
    min-width: 0;
    color: var(--text-2);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.shell .title {
    font-weight: 400;
  }
  .title:not(:has(+ .action)) {
    flex: 1;
  }
  .age {
    flex: none;
    margin-left: auto;
    font-size: 11px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .chev {
    flex: none;
    color: var(--text-2);
  }
  :global(:root.phone) .row {
    height: var(--touch);
    gap: 10px;
    margin-left: 12px;
    width: calc(100% - 12px);
    padding: 0 10px;
    border-radius: 10px;
  }
  @media (hover: none) {
    .wrap.agent .note {
      opacity: 1;
    }
    .wrap.agent .nslot {
      display: block;
    }
    .wrap.agent:not(.noted) .age {
      visibility: visible;
    }
  }
  :global(:root.phone) .note {
    right: 28px;
    width: 32px;
    height: 32px;
    opacity: 1;
  }
  :global(:root.phone) .wrap.agent .age {
    visibility: visible;
  }
  :global(:root.phone) .wrap.agent .nslot {
    display: block;
    width: 26px;
  }
  :global(:root.phone) .row.shell {
    height: 40px;
  }
  :global(:root.phone) .action,
  :global(:root.phone) .age {
    font-size: 13px;
  }
  :global(:root.phone) .chev {
    font-size: 18px;
  }
</style>
