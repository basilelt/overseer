<script lang="ts">
  import type { Component } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import Server from '@lucide/svelte/icons/server';
  import FolderTree from '@lucide/svelte/icons/folder-tree';
  import Megaphone from '@lucide/svelte/icons/megaphone';
  import Box from '@lucide/svelte/icons/box';
  import X from '@lucide/svelte/icons/x';
  import { t, type Key } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { highlight, rank, type Match } from '../../fuzzy';
  import type { GroupId } from '../../grouping';
  import { PALETTE_GROUP_COLOR, PALETTE_HELD_COLOR } from '../../ws-color';
  import { canInstall, ordered } from '../../agents';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import { isPhone } from '../../mobile/ui.svelte';
  import { phoneOrMobile } from '../../mobile/phone.svelte';
  import { broadcastTo } from '../../wsactions';
  import { isGhost } from '../../workspaces';
  import { command, has } from '../../commands';
  import { registerMore } from '../../commands-more';
  import { hint } from '../../keyboard';
  import { hostName } from '../../hosts';

  registerMore();

  let { onclose }: { onclose: () => void } = $props();

  interface Cmd {
    id: string;
    label: string;
    hint?: string;
    group?: GroupId;
    icon?: Component<{ size?: number }>;
    agent?: string;
    section: 'sessions' | 'views' | 'actions';
    keys?: string;
    run: () => unknown;
  }

  const connected = app.connected;
  const desktop = !phoneOrMobile();
  const UNHINTED = new Set(['source-control']);

  const entry = (id: string): Cmd => {
    const c = command(id);
    return {
      id: c.id,
      label: c.label?.() ?? c.id,
      icon: c.icon,
      section: c.section ?? 'actions',
      keys: desktop && c.shortcut && !UNHINTED.has(c.id) ? hint(c.shortcut) || undefined : undefined,
      run: () => {
        if (c.agentsView) app.view = 'agents';
        return c.run();
      },
    };
  };
  const listed = (...ids: string[]): Cmd[] => ids.filter((id) => has(id) && command(id).available()).map(entry);

  const sessionActions: Cmd[] = connected
    ? [
        ...ordered(app.agents)
          .filter((a) => a.installed)
          .map((a): Cmd => ({ id: `new-${a.id}`, label: t('palette.newSession', { name: a.name }), agent: a.id, section: 'actions', run: () => app.newSession(a.id) })),
        ...app.agents
          .filter(canInstall)
          .map((a): Cmd => ({ id: `install-${a.id}`, label: t('palette.installAgent', { name: a.name }), agent: a.id, section: 'actions', run: () => app.installAgent(a.id).catch((e) => app.notify(String((e as Error)?.message ?? e), 'error')) })),
      ]
    : [];

  // The current workspace, or the parent of the current nested one, takes new subjects.
  const subjectParent = $derived.by(() => {
    const w = app.currentWorkspace;
    const top = w?.parentId ? app.ws.workspaces.find((p) => p.id === w.parentId) : w;
    return top && top.path && top.kind !== 'subject' && !top.parentId ? top : undefined;
  });
  const subjectAction = (): Cmd[] =>
    connected && app.wsPersistent && subjectParent
      ? [{ id: 'subject-new', label: t('palette.newSubject'), icon: FolderTree, section: 'actions', run: () => app.open('workspaceNew', subjectParent.id) }]
      : [];
  const views: Cmd[] = listed('review', 'review-beside', 'browser', 'graph', 'worktrees', 'system', 'usage', 'prs');
  const actions: Cmd[] = [
    ...listed('review-close'),
    ...sessionActions,
    ...listed('kanban', 'agents', 'connect'),
    ...(desktop ? listed('add-host') : []),
    ...app.others.map((l): Cmd => ({ id: `host:${l.key}`, label: t('palette.showHost', { name: hostName(l.target) }), icon: Server, section: 'actions', run: () => app.switchHost(l.key) })),
    ...listed('disconnect', 'theme', 'panel', 'live-review', 'source-control', 'split-right', 'split-down', 'settings', 'skills', 'logs', 'feedback', 'record', 'tour', 'organize', 'ws-new', 'folder-new', 'rescan-folders'),
    ...subjectAction(),
    ...listed('ws-broadcast', 'ws-hold', 'ws-resume', 'group-new', 'sel-broadcast'),
    ...app.ws.groups.map((g): Cmd => ({ id: `group-bc-${g.id}`, label: t('palette.broadcastGroup', { name: g.name }), icon: Megaphone, section: 'actions', run: () => broadcastTo({ kind: 'group', id: g.id }) })),
    ...app.ws.workspaces
      .filter((w) => !w.archived && !isGhost(w))
      .map((w): Cmd => ({
        id: `ws-${w.id}`,
        label: t('palette.goWorkspace', { name: w.name }),
        icon: Box,
        section: 'actions',
        run: () => {
          if (app.sidebarMode !== 'workspaces') app.setSidebarMode('workspaces');
          app.selectWorkspace(w.id);
        },
      })),
    ...listed('ports', 'forward-port', 'updates', 'apk-updates'),
    ...(desktop ? listed('shortcuts', 'new-window', 'sidebar', ...(app.view === 'agents' ? ['tab-new', 'tab-close', 'tab-reopen'] : [])) : []),
  ];

  let query = $state('');
  let active = $state(0);
  let list: HTMLElement | undefined = $state();

  const sessions = $derived<Cmd[]>(
    app.items.map((i) => ({
      id: i.key,
      label: i.title,
      hint: t(`group.${i.group}` as Key),
      group: i.group,
      agent: i.kind,
      section: 'sessions',
      run: () => app.select(i.key),
    })),
  );
  const results = $derived<{ item: Cmd; match: Match }[]>([
    ...rank(query, sessions, (c) => c.label).slice(0, query ? 8 : 5),
    ...rank(query, views, (c) => c.label),
    ...rank(query, actions, (c) => c.label),
  ]);

  $effect(() => {
    query;
    active = 0;
  });

  $effect(() => {
    list?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  function run(c: Cmd | undefined) {
    if (!c) return;
    onclose();
    Promise.resolve(c.run()).catch((e) => app.notify(String(e?.message ?? e), 'error'));
  }

  function onkeydown(e: KeyboardEvent) {
    const n = results.length;
    if (e.key === 'ArrowDown') active = n ? (active + 1) % n : 0;
    else if (e.key === 'ArrowUp') active = n ? (active - 1 + n) % n : 0;
    else if (e.key === 'Enter') run(results[active]?.item);
    else if (e.key === 'Escape') onclose();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  const focus = (el: HTMLInputElement) => el.focus();
</script>

<div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="palette" role="dialog" aria-modal="true" aria-label={t('title.palette')}>
    <label class="search">
      <Search size={15} />
      <input
        use:focus
        bind:value={query}
        {onkeydown}
        placeholder={t('palette.placeholder')}
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={results.length ? `palette-${active}` : undefined}
      />
      <kbd>esc</kbd>
      {#if isPhone()}<button class="icon-btn" aria-label={t('common.close')} onclick={onclose}><X size={18} /></button>{/if}
    </label>
    <ul id="palette-list" role="listbox" bind:this={list}>
      {#each results as { item, match }, i (item.id)}
        {#if i === 0 || results[i - 1].item.section !== item.section}
          <li class="section" role="presentation">{t(`palette.section.${item.section}`)}</li>
        {/if}
        <li
          id="palette-{i}"
          data-i={i}
          role="option"
          aria-selected={i === active}
          class:active={i === active}
          onmousemove={() => (active = i)}
          onclick={() => run(item)}
          onkeydown={() => {}}
        >
          <span class="icon">{#if item.agent}<AgentGlyph agent={item.agent} size={14} />{:else if item.icon}<item.icon size={14} />{/if}</span>
          <span class="label">
            {#each highlight(item.label, match.ranges) as part}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
          </span>
          {#if item.keys}<kbd class="keys">{item.keys}</kbd>{/if}
          {#if item.hint}
            <span class="hint"><span class="dot" style:background={item.group ? PALETTE_GROUP_COLOR[item.group] : PALETTE_HELD_COLOR}></span>{item.hint}</span>
          {/if}
        </li>
      {:else}
        <li class="empty">{t('palette.empty')}</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 14vh;
    z-index: 60;
  }
  .palette {
    width: 580px;
    max-width: calc(100vw - 32px);
    background: var(--surface);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    color: var(--text-3);
    border-bottom: 1px solid var(--border);
  }
  .search input {
    flex: 1;
    border: 0;
    background: transparent;
    padding: 13px 0;
    font-size: 14px;
    color: var(--text);
  }
  kbd {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--text-3);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 5px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 6px;
    max-height: 52vh;
    overflow: auto;
  }
  li[role='option'] {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius);
    cursor: pointer;
    color: var(--text);
  }
  li.active {
    background: var(--accent-soft);
  }
  .section {
    font-size: 11px;
    font-weight: 500;
    color: var(--text-3);
    padding: 8px 10px 4px;
  }
  .icon {
    display: grid;
    color: var(--text-2);
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  mark {
    background: none;
    color: var(--accent);
    font-weight: 600;
  }
  kbd.keys {
    flex: none;
    white-space: nowrap;
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-3);
  }
  .empty {
    padding: 18px 10px;
    color: var(--text-3);
    text-align: center;
  }
  :global(.phone) .backdrop {
    padding-top: 0;
    align-items: stretch;
    background: var(--surface);
  }
  :global(.phone) .palette {
    width: 100%;
    max-width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    border-radius: 0;
    box-shadow: none;
    padding: var(--safe-top) var(--safe-right) var(--safe-bottom) var(--safe-left);
  }
  :global(.phone) .search input {
    min-height: 52px;
    border: 0;
    font-size: 16px;
  }
  :global(.phone) kbd {
    display: none;
  }
  :global(.phone) ul {
    flex: 1;
    max-height: none;
  }
  :global(.phone) li[role='option'] {
    height: 48px;
    font-size: 15px;
  }
</style>
