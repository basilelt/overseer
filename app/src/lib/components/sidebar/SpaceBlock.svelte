<script lang="ts" module>
  import type { Workspace } from '../../types';

  /** What a block needs to show and act on its nested workspaces. */
  export interface ChildHooks {
    isOpen: (id: string) => boolean;
    isClosedOpen: (id: string) => boolean;
    selectedId: string;
    tabDrop: string | null;
    sessionDrop: string | null;
    ontoggle: (id: string) => void;
    onpick: (id: string) => void;
    onclosed: (id: string) => void;
    onmenu: (e: MouseEvent, w: Workspace) => void;
    onstart: (e: MouseEvent, w: Workspace) => void;
    onnew?: () => void;
  }
</script>

<script lang="ts">
  import FolderIcon from '@lucide/svelte/icons/folder';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import Hash from '@lucide/svelte/icons/hash';
  import Plus from '@lucide/svelte/icons/plus';
  import { t } from '../../i18n.svelte';
  import type { SpaceRow } from '../../spaces';
  import { workspaceSessions, type WorkspaceSessions } from '../../workspace-sessions';
  import type { ConnState } from '../../types';
  import SessionRow from './SessionRow.svelte';
  import StatePills from './StatePills.svelte';
  import WorkspaceFrame from './WorkspaceFrame.svelte';

  let {
    row,
    now,
    open,
    closedOpen,
    selectedKey,
    multi,
    host,
    selected = false,
    tabDrop = false,
    dropPos = null,
    kids,
    ontoggle,
    onpick,
    onclosed,
    onselect,
    onmenu,
    onitemmenu,
    onpointerdown,
    onsessiondown,
  }: {
    row: SpaceRow;
    now: number;
    open: boolean;
    closedOpen: boolean;
    selectedKey: string | null;
    multi?: ReadonlySet<string>;
    host?: { name: string } & Pick<ConnState, 'phase' | 'attempt' | 'version'>;
    /** This workspace is the active one. */
    selected?: boolean;
    /** A dragged tab hovers the header. */
    tabDrop?: boolean;
    dropPos?: 'before' | 'after' | 'inside' | null;
    kids?: ChildHooks;
    /** The chevron only folds; the name picks the workspace. */
    ontoggle: () => void;
    onpick: () => void;
    onclosed: () => void;
    onselect: (key: string, e?: MouseEvent) => void;
    onmenu?: (e: MouseEvent) => void;
    onitemmenu?: (key: string, title: string, e: MouseEvent) => void;
    onpointerdown?: (e: PointerEvent) => void;
    onsessiondown?: (e: PointerEvent, key: string) => void;
  } = $props();

  const sessions = $derived(open ? workspaceSessions(row.items, now) : null);
  const name = $derived(row.name ?? t('ws.other'));
  const archived = $derived(!!row.workspace?.archived);
  const missing = $derived(!!row.workspace?.missing);
  // "Other" has no id: it cannot receive a tab or be dragged.
  const id = $derived(row.workspace ? row.id : undefined);
  const children = $derived(open ? (row.children ?? []) : []);

  const kindOf = (w: Workspace) => (w.kind === 'subject' ? 'subject' : w.auto ? 'submodule' : 'folder');
</script>

{#snippet line()}
  <div
    class="line"
    class:selected
    class:archived
    class:missing
    class:tab-drop={tabDrop}
    class:drop-before={dropPos === 'before'}
    class:drop-after={dropPos === 'after'}
    class:drop-inside={dropPos === 'inside'}
    data-workspace-id={id}
    data-drop={id ? 'workspace' : undefined}
    data-id={id}
    role="presentation"
    oncontextmenu={(e) => onmenu && (e.preventDefault(), onmenu(e))}
    {onpointerdown}
  >
    <button class="chev" type="button" aria-expanded={open} aria-label={t(open ? 'ws.collapse' : 'ws.expand', { name })} onclick={ontoggle}><span class:open>›</span></button>
    <button class="pick" type="button" aria-current={selected ? 'true' : undefined} onclick={onpick}>
      <span class="name">{name}</span>
      {#if missing}<span class="gone" title={row.workspace?.path ?? undefined}>{t('ws.missing')}</span>{/if}
      <StatePills summary={row.summary} />
    </button>
  </div>
{/snippet}

{#snippet list(s: WorkspaceSessions, key: string, isClosedOpen: boolean, toggleClosed: () => void)}
  {#each s.live as it (it.key)}
    <SessionRow
      item={it}
      {now}
      {host}
      selected={selectedKey === it.key}
      multi={multi?.has(it.key)}
      onselect={(e) => onselect(it.key, e)}
      onmenu={onitemmenu && ((e) => onitemmenu(it.key, it.title, e))}
      onpointerdown={onsessiondown && ((e) => onsessiondown(e, it.key))}
    />
  {/each}
  {#each s.shells as it (it.key)}
    <SessionRow item={it} {now} {host} selected={selectedKey === it.key} multi={multi?.has(it.key)} onselect={(e) => onselect(it.key, e)} onmenu={onitemmenu && ((e) => onitemmenu(it.key, it.title, e))} onpointerdown={onsessiondown && ((e) => onsessiondown(e, it.key))} />
  {/each}
  {#if s.closed.count}
    <button class="more" type="button" data-closed={key} aria-expanded={isClosedOpen} onclick={toggleClosed}>{s.closed.count === 1 ? t('ws.closedOne') : t('ws.closedMore', { n: s.closed.count })}</button>
    {#if isClosedOpen}
      {#each s.closed.buckets as b (b.bucket)}
        <div class="bucket">{t(`ws.bucket.${b.bucket}`)}</div>
        {#each b.items as it (it.key)}
          <SessionRow item={it} {now} {host} selected={selectedKey === it.key} multi={multi?.has(it.key)} onselect={(e) => onselect(it.key, e)} onmenu={onitemmenu && ((e) => onitemmenu(it.key, it.title, e))} />
        {/each}
      {/each}
    {/if}
  {/if}
{/snippet}

{#if sessions}
  <WorkspaceFrame header={line}>
    {@render list(sessions, row.id, closedOpen, onclosed)}
    {#if kids}
      {#each children as c (c.id)}
        {@const w = c.workspace!}
        {@const copen = kids.isOpen(c.id) && c.items.length > 0}
        {@const kind = kindOf(w)}
        <div
          class="line child"
          class:selected={kids.selectedId === c.id}
          class:idle={!c.items.length}
          class:missing={!!w.missing}
          class:tab-drop={kids.tabDrop === c.id || kids.sessionDrop === c.id}
          data-workspace-id={c.id}
          data-drop="child"
          data-id={c.id}
          data-kind={kind}
          role="presentation"
          oncontextmenu={(e) => (e.preventDefault(), kids.onmenu(e, w))}
        >
          <button class="chev" type="button" aria-expanded={copen} disabled={!c.items.length} aria-label={t(copen ? 'ws.collapse' : 'ws.expand', { name: c.name ?? '' })} onclick={() => kids.ontoggle(c.id)}
            ><span class:open={copen}>›</span></button
          >
          <button class="pick" type="button" aria-current={kids.selectedId === c.id ? 'true' : undefined} title={w.path ?? w.cwd ?? undefined} onclick={() => kids.onpick(c.id)}>
            <span class="kind" title={t(kind === 'submodule' ? 'ws.sub.submodule' : kind === 'subject' ? 'ws.sub.subject' : 'ws.sub.folder')}>
              {#if kind === 'submodule'}<GitBranch size={12} />{:else if kind === 'subject'}<Hash size={12} />{:else}<FolderIcon size={12} />{/if}
            </span>
            <span class="name">{c.name}</span>
            {#if w.missing}<span class="gone">{t('ws.missing')}</span>{/if}
            <StatePills summary={c.summary} />
          </button>
          <button class="start" type="button" title={t('ws.sub.startHere', { name: c.name ?? '' })} aria-label={t('ws.sub.startHere', { name: c.name ?? '' })} onclick={(e) => kids.onstart(e, w)}><Plus size={12} /></button>
        </div>
        {#if copen}
          <div class="kids">{@render list(workspaceSessions(c.items, now), c.id, kids.isClosedOpen(c.id), () => kids.onclosed(c.id))}</div>
        {/if}
      {/each}
      {#if kids.onnew}<button class="more new" type="button" data-tour="subjects" onclick={kids.onnew}>{t('ws.sub.add')}</button>{/if}
    {/if}
  </WorkspaceFrame>
{:else}
  <div class="solo">{@render line()}</div>
{/if}

<style>
  .solo {
    margin: 0 6px;
  }
  .line {
    position: relative;
    display: flex;
    align-items: center;
    gap: 2px;
    width: 100%;
    height: 28px;
    border-radius: 6px;
    min-width: 0;
    font-weight: 600;
  }
  .line.selected {
    background: var(--accent-soft);
  }
  .line.tab-drop,
  .line.drop-inside {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .line.missing .name {
    color: var(--text-3);
  }
  .gone {
    flex: none;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-3);
  }
  .line.archived {
    opacity: 0.55;
  }
  .child {
    padding-left: 10px;
    font-weight: 500;
  }
  .child:hover {
    background: var(--surface-hover);
  }
  .child.selected {
    background: var(--accent-soft);
  }
  .child.idle .name {
    color: var(--text-3);
  }
  .kind {
    display: inline-grid;
    flex: none;
    color: var(--text-3);
  }
  .start {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: 20px;
    height: 20px;
    margin-right: 4px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text-3);
    opacity: 0;
  }
  .child:hover .start,
  .start:focus-visible {
    opacity: 1;
  }
  .start:hover {
    background: var(--surface-active);
    color: var(--text);
  }
  @media (hover: none) {
    .start {
      opacity: 1;
    }
  }
  .kids {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding-left: 10px;
  }
  .drop-before::before,
  .drop-after::after {
    content: '';
    position: absolute;
    left: 8px;
    right: 8px;
    height: 2px;
    background: var(--accent);
  }
  .drop-before::before {
    top: -1px;
  }
  .drop-after::after {
    bottom: -1px;
  }
  .chev,
  .pick {
    display: flex;
    align-items: center;
    height: 100%;
    border: 0;
    background: none;
    font-weight: inherit;
    text-align: left;
  }
  .chev {
    flex: none;
    width: 20px;
    justify-content: center;
    padding: 0;
    color: var(--text-2);
    border-radius: 6px;
  }
  .chev:disabled {
    color: var(--text-3);
    cursor: default;
  }
  .chev span {
    display: inline-block;
  }
  .chev .open {
    transform: rotate(90deg);
  }
  .pick {
    flex: 1;
    gap: 6px;
    min-width: 0;
    padding: 0 6px 0 0;
    border-radius: 6px;
  }
  .solo .line:hover {
    background: var(--surface-hover);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .more {
    margin-left: 20px;
    height: 24px;
    padding: 0 8px;
    border: 0;
    background: none;
    color: var(--text-2);
    font-size: 12px;
    text-align: left;
    border-radius: 6px;
  }
  .more:hover {
    background: var(--surface-hover);
  }
  .bucket {
    margin-left: 20px;
    padding: 2px 8px 0;
    font-size: 11px;
    color: var(--text-2);
  }
  :global(:root.phone) .line {
    height: var(--touch);
    border-radius: 10px;
  }
  :global(:root.phone) .chev {
    width: 36px;
    font-size: 18px;
    border-radius: 10px;
  }
  :global(:root.phone) .child {
    padding-left: 0;
  }
  :global(:root.phone) .start {
    width: 40px;
    height: 40px;
    margin-right: 2px;
    border-radius: 10px;
  }
  :global(:root.phone) .more {
    height: 40px;
    margin-left: 12px;
    padding: 0 10px;
    font-size: 14px;
    border-radius: 10px;
  }
  :global(:root.phone) .bucket {
    margin-left: 12px;
    padding: 6px 10px 2px;
    font-size: 12px;
  }
</style>
