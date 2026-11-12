<script lang="ts">
  import Gauge from '@lucide/svelte/icons/gauge';
  import { ust } from '../../usage/strings';
  import GitPullRequestArrow from '@lucide/svelte/icons/git-pull-request-arrow';
  import { gt } from '../../github/strings';
  import { openPrReviews } from '../../github/open.svelte';
  import type { Component } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import Unplug from '@lucide/svelte/icons/unplug';
  import KanbanSquare from '@lucide/svelte/icons/square-kanban';
  import AppWindow from '@lucide/svelte/icons/app-window';
  import GitGraph from '@lucide/svelte/icons/git-graph';
  import FolderTree from '@lucide/svelte/icons/folder-tree';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import Globe from '@lucide/svelte/icons/globe';
  import Server from '@lucide/svelte/icons/server';
  import SunMoon from '@lucide/svelte/icons/sun-moon';
  import PanelRight from '@lucide/svelte/icons/panel-right';
  import Puzzle from '@lucide/svelte/icons/puzzle';
  import Settings from '@lucide/svelte/icons/settings';
  import Columns2 from '@lucide/svelte/icons/columns-2';
  import Rows2 from '@lucide/svelte/icons/rows-2';
  import EthernetPort from '@lucide/svelte/icons/ethernet-port';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Activity from '@lucide/svelte/icons/activity';
  import { st } from '../../system/strings';
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import MessageSquarePlus from '@lucide/svelte/icons/message-square-plus';
  import Compass from '@lucide/svelte/icons/compass';
  import WandSparkles from '@lucide/svelte/icons/wand-sparkles';
  import { organizer } from '../../organize.svelte';
  import { tt } from '../../tour/strings';
  import { replayTour } from '../../tour/replay';
  import { feedback, ft } from '../../feedback.svelte';
  import { diag, dt } from '../../diag.svelte';
  import { updater, ut } from '../../updater.svelte';
  import { apk, at } from '../../mobile/apk-update.svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { saveSettings, settings, type ThemePref } from '../../settings.svelte';
  import { highlight, rank, type Match } from '../../fuzzy';
  import type { GroupId } from '../../grouping';
  import { ordered } from '../../agents';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import Megaphone from '@lucide/svelte/icons/megaphone';
  import Pause from '@lucide/svelte/icons/pause';
  import Play from '@lucide/svelte/icons/play';
  import Users from '@lucide/svelte/icons/users';
  import Box from '@lucide/svelte/icons/box';
  import X from '@lucide/svelte/icons/x';
  import { ui } from '../../mobile/ui.svelte';
  import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
  import { closeSideReview, openReview, openReviewBeside, reviewTargetFor } from '../../review/open';
  import { beside } from '../../review/beside.svelte';
  import { broadcastTo, createFolder, createGroup, holdAll, rescanFolders } from '../../wsactions';
  import { isGhost } from '../../workspaces';
  import Keyboard from '@lucide/svelte/icons/keyboard';
  import PanelLeft from '@lucide/svelte/icons/panel-left';
  import Plus from '@lucide/svelte/icons/plus';
  import X2 from '@lucide/svelte/icons/square-x';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import { hint, newWindow, runShortcut } from '../../keyboard';
  import { hostName } from '../../hosts';
  import type { ShortcutId } from '../../shortcuts';

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

  const split = (dir: 'horizontal' | 'vertical') => {
    app.view = 'agents';
    window.dispatchEvent(new CustomEvent('sb:split', { detail: dir }));
  };
  const nextTheme: Record<ThemePref, ThemePref> = { auto: 'light', light: 'dark', dark: 'auto' };
  const action = (id: string, key: Key, icon: Cmd['icon'], run: () => unknown): Cmd => ({ id, label: t(key), icon, section: 'actions', run });
  const view = (id: string, key: Key, icon: Cmd['icon'], run: () => unknown): Cmd => ({ ...action(id, key, icon, run), section: 'views' });
  const connected = app.connected;

  const sessionActions: Cmd[] = connected
    ? ordered(app.agents)
        .filter((a) => a.installed)
        .map((a) => ({ id: `new-${a.id}`, label: t('palette.newSession', { name: a.name }), agent: a.id, section: 'actions', run: () => app.newSession(a.id) }))
    : [];

  const reviewChanges = (side = false) => {
    const target = reviewTargetFor(app.selected);
    if (target) (side ? openReviewBeside : openReview)(target);
    else app.notify(t('review.noLine'), 'error');
  };
  const desktop = !app.mobile && ui.layout !== 'phone';
  const KEYED: Record<string, ShortcutId> = {
    kanban: 'kanban',
    panel: 'togglePanel',
    'split-right': 'splitRight',
    'split-down': 'splitDown',
    settings: 'settings',
    'ws-new': 'newWorkspace',
    'new-window': 'newWindow',
    sidebar: 'toggleSidebar',
    shortcuts: 'shortcuts',
    'tab-new': 'newTab',
    'tab-close': 'closeTab',
    'tab-reopen': 'reopenTab',
  };
  const tabAction = (id: string, key: Key, icon: Cmd['icon'], sc: ShortcutId): Cmd =>
    action(id, key, icon, () => {
      app.view = 'agents';
      runShortcut({ id: sc });
    });
  const desktopActions: Cmd[] = desktop
    ? [
        action('shortcuts', 'palette.shortcuts', Keyboard, () => app.open('shortcuts')),
        action('new-window', 'palette.newWindow', AppWindow, () => newWindow()),
        action('sidebar', 'palette.toggleSidebar', PanelLeft, () => (app.sidebarOpen = !app.sidebarOpen)),
        ...(app.view === 'agents' ? [tabAction('tab-new', 'palette.newTab', Plus, 'newTab'), tabAction('tab-close', 'palette.closeTab', X2, 'closeTab'), tabAction('tab-reopen', 'palette.reopenTab', Undo2, 'reopenTab')] : []),
      ]
    : [];
  const views: Cmd[] = [
    ...(connected ? [view('review', 'review.palette', GitPullRequest, () => reviewChanges()), view('review-beside', 'review.beside', GitPullRequest, () => reviewChanges(true))] : []),
    view('browser', 'palette.openBrowser', Globe, () => (app.view = 'browser')),
    view('graph', 'palette.openGit', GitGraph, () => app.openProjectTab('graph')),
    view('worktrees', 'palette.openWorktrees', FolderTree, () => app.openProjectTab('worktrees')),
    ...(connected ? [{ id: 'system', label: st('open'), icon: Activity, section: 'views', run: () => app.open('system') } as Cmd] : []),
    ...(connected ? [{ id: 'usage', label: ust('open'), icon: Gauge, section: 'views', run: () => app.open('usage') } as Cmd] : []),
    ...(connected ? [{ id: 'prs', label: gt('open'), icon: GitPullRequestArrow, section: 'views', run: () => openPrReviews() } as Cmd] : []),
  ];
  const actions: Cmd[] = ([
    ...(beside.key ? [action('review-close', 'review.close', GitPullRequest, () => closeSideReview())] : []),
    ...sessionActions,
    action('kanban', 'palette.openKanban', KanbanSquare, () => (app.view = 'kanban')),
    action('agents', 'palette.openAgents', LayoutGrid, () => (app.view = 'agents')),
    action('connect', connected ? 'palette.switchHost' : 'palette.connectHost', Server, () => app.open('connect')),
    ...((connected || app.hosts.length > 1) && desktop ? [action('add-host', 'palette.addHost', Server, () => app.open('connect', 'add'))] : []),
    ...app.others.map((l) => ({ id: `host:${l.key}`, label: t('palette.showHost', { name: hostName(l.target) }), icon: Server, section: 'actions', run: () => app.switchHost(l.key) }) as Cmd),
    ...(connected ? [action('disconnect', 'palette.disconnect', Unplug, () => app.disconnect())] : []),
    action('theme', 'palette.toggleTheme', SunMoon, () => saveSettings({ theme: nextTheme[settings.theme] })),
    action('panel', 'palette.togglePanel', PanelRight, () => (app.panelOpen = !app.panelOpen)),
    action('split-right', 'palette.splitRight', Columns2, () => split('horizontal')),
    action('split-down', 'palette.splitDown', Rows2, () => split('vertical')),
    action('settings', 'palette.settings', Settings, () => app.open('settings')),
    ...(connected ? [action('skills', 'palette.skills', Puzzle, () => app.open('settings', 'skills'))] : []),
    { id: 'logs', label: dt('show'), icon: ScrollText, section: 'actions', run: () => diag.open('live') },
    { id: 'feedback', label: ft('title'), icon: MessageSquarePlus, section: 'actions', run: () => feedback.show() },
    { id: 'tour', label: tt('palette'), icon: Compass, section: 'actions', run: () => replayTour() },
    ...(connected ? [{ id: 'organize', label: t('org.palette'), icon: WandSparkles, section: 'actions', run: () => organizer.show() } as Cmd] : []),
    ...(connected && app.wsPersistent
      ? [action('ws-new', 'palette.newWorkspace', FolderTree, () => app.open('workspaceNew')), action('folder-new', 'palette.newFolder', FolderPlus, () => createFolder())]
      : []),
    ...(connected ? [action('rescan-folders', 'palette.rescanFolders', RefreshCw, () => rescanFolders())] : []),
    ...(connected && app.currentWorkspace
      ? [
          action('ws-broadcast', 'palette.broadcastWorkspace', Megaphone, () => broadcastTo({ kind: 'workspace', id: app.workspaceId })),
          action('ws-hold', 'palette.holdWorkspace', Pause, () => holdAll({ kind: 'workspace', id: app.workspaceId }, 'hold')),
          action('ws-resume', 'palette.resumeWorkspace', Play, () => holdAll({ kind: 'workspace', id: app.workspaceId }, 'resume')),
        ]
      : []),
    ...(connected && app.multi.size
      ? [
          action('group-new', 'palette.groupFromSelection', Users, () => createGroup([...app.multi])),
          action('sel-broadcast', 'palette.broadcastSelection', Megaphone, () => broadcastTo({ kind: 'items', keys: [...app.multi] })),
        ]
      : []),
    ...app.ws.groups.map((g) => ({ id: `group-bc-${g.id}`, label: t('palette.broadcastGroup', { name: g.name }), icon: Megaphone, section: 'actions' as const, run: () => broadcastTo({ kind: 'group', id: g.id }) })),
    ...app.ws.workspaces
      .filter((w) => !w.archived && !isGhost(w))
      .map((w) => ({
        id: `ws-${w.id}`,
        label: t('palette.goWorkspace', { name: w.name }),
        icon: Box,
        section: 'actions' as const,
        run: () => {
          if (app.sidebarMode !== 'workspaces') app.setSidebarMode('workspaces');
          app.selectWorkspace(w.id);
        },
      })),
    ...(connected ? [action('ports', 'palette.ports', EthernetPort, () => app.open('ports'))] : []),
    ...(connected && app.conn.target.kind !== 'local' ? [action('forward-port', 'palette.forwardPort', EthernetPort, () => app.open('ports', 'forward'))] : []),
    ...(updater.enabled && !app.mobile ? [{ id: 'updates', label: ut('check'), icon: RefreshCw, section: 'actions' as const, run: () => updater.check(true) }] : []),
    ...(__SELF_UPDATE__ && apk.enabled && apk.updates ? [{ id: 'apk-updates', label: at('check'), icon: RefreshCw, section: 'actions' as const, run: () => apk.check(true) }] : []),
    ...desktopActions,
  ] as Cmd[]).map((c) => (desktop && KEYED[c.id] ? { ...c, keys: hint(KEYED[c.id]) || undefined } : c));

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
      {#if ui.layout === 'phone'}<button class="icon-btn" aria-label={t('common.close')} onclick={onclose}><X size={18} /></button>{/if}
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
            <span class="hint"><span class="dot" style:background="var(--st-{item.group === 'needs_input' ? 'input' : item.group === 'running' ? 'working' : item.group === 'needs_testing' ? 'testing' : item.group === 'paused' ? 'paused' : 'held'})"></span>{item.hint}</span>
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
