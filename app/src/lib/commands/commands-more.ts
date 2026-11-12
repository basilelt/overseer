import AppWindow from '@lucide/svelte/icons/app-window';
import Columns2 from '@lucide/svelte/icons/columns-2';
import FolderPlus from '@lucide/svelte/icons/folder-plus';
import KanbanSquare from '@lucide/svelte/icons/square-kanban';
import Keyboard from '@lucide/svelte/icons/keyboard';
import PanelLeft from '@lucide/svelte/icons/panel-left';
import PanelRight from '@lucide/svelte/icons/panel-right';
import Plus from '@lucide/svelte/icons/plus';
import Rows2 from '@lucide/svelte/icons/rows-2';
import Search from '@lucide/svelte/icons/search';
import Settings from '@lucide/svelte/icons/settings';
import Undo2 from '@lucide/svelte/icons/undo-2';
import X2 from '@lucide/svelte/icons/square-x';
import Activity from '@lucide/svelte/icons/activity';
import Bell from '@lucide/svelte/icons/bell';
import Clapperboard from '@lucide/svelte/icons/clapperboard';
import Compass from '@lucide/svelte/icons/compass';
import EthernetPort from '@lucide/svelte/icons/ethernet-port';
import FolderTree from '@lucide/svelte/icons/folder-tree';
import Gauge from '@lucide/svelte/icons/gauge';
import GitGraph from '@lucide/svelte/icons/git-graph';
import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
import GitPullRequestArrow from '@lucide/svelte/icons/git-pull-request-arrow';
import Globe from '@lucide/svelte/icons/globe';
import LayoutGrid from '@lucide/svelte/icons/layout-grid';
import Megaphone from '@lucide/svelte/icons/megaphone';
import MessageSquarePlus from '@lucide/svelte/icons/message-square-plus';
import Pause from '@lucide/svelte/icons/pause';
import Play from '@lucide/svelte/icons/play';
import Puzzle from '@lucide/svelte/icons/puzzle';
import RefreshCw from '@lucide/svelte/icons/refresh-cw';
import ScrollText from '@lucide/svelte/icons/scroll-text';
import Server from '@lucide/svelte/icons/server';
import SunMoon from '@lucide/svelte/icons/sun-moon';
import Unplug from '@lucide/svelte/icons/unplug';
import Users from '@lucide/svelte/icons/users';
import WandSparkles from '@lucide/svelte/icons/wand-sparkles';
import { allCommands, connected, register, type CommandDef, type CommandIcon, type CoreId } from './commands';
import { diag, dt } from '../diag/diag.svelte';
import { feedback, ft } from '../feedback/feedback.svelte';
import { toggleLiveReview } from '../git/live-actions';
import { openPrReviews } from '../github/open.svelte';
import { gt } from '../github/strings';
import { t, type Key } from '../i18n.svelte';
import { apk, at } from '../mobile/apk-update.svelte';
import { organizer } from '../workspace/organize.svelte';
import { recorder, rt } from '../record/record.svelte';
import { beside } from '../review/beside.svelte';
import { closeSideReview, openReview, openReviewBeside, reviewTargetFor } from '../review/open';
import { saveSettings, settings } from '../settings/settings.svelte';
import type { ThemePref } from '../theme/theme';
import { app } from '../state.svelte';
import { st } from '../system/strings';
import { replayTour } from '../tour/replay';
import { tt } from '../tour/strings';
import { ust } from '../usage/strings';
import { updater, ut } from '../updater.svelte';
import { broadcastTo, createGroup, holdAll } from '../workspace/wsactions';

const label = (key: Key) => () => t(key);

const CORE_LABELS: Record<CoreId, Key> = {
  'kanban': 'palette.openKanban',
  'panel': 'palette.togglePanel',
  'source-control': 'palette.sourceControl',
  'split-right': 'palette.splitRight',
  'split-down': 'palette.splitDown',
  'settings': 'palette.settings',
  'ws-new': 'palette.newWorkspace',
  'folder-new': 'palette.newFolder',
  'rescan-folders': 'palette.rescanFolders',
  'shortcuts': 'palette.shortcuts',
  'new-window': 'palette.newWindow',
  'sidebar': 'palette.toggleSidebar',
  'tab-new': 'palette.newTab',
  'tab-close': 'palette.closeTab',
  'tab-reopen': 'palette.reopenTab',
  'palette': 'title.palette',
  'reconnect': 'keys.reconnect',
  'tab-next': 'keys.nextTab',
  'tab-prev': 'keys.prevTab',
  'tab-index': 'keys.tabIndex',
  'focus-left': 'keys.focusLeft',
  'focus-right': 'keys.focusRight',
  'focus-up': 'keys.focusUp',
  'focus-down': 'keys.focusDown',
  'term-find': 'keys.find',
  'term-copy': 'keys.copy',
  'term-paste': 'keys.paste',
  'font-up': 'keys.fontUp',
  'font-down': 'keys.fontDown',
  'font-reset': 'keys.fontReset',
  'close-window': 'keys.closeWindow',
};

const AGENTS_VIEW = new Set<CoreId>(['split-right', 'split-down', 'tab-new', 'tab-close', 'tab-reopen']);

function reviewChanges(side = false) {
  const target = reviewTargetFor(app.selected);
  if (target) (side ? openReviewBeside : openReview)(target);
  else app.notify(t('review.noLine'), 'error');
}

const nextTheme: Record<ThemePref, ThemePref> = { auto: 'light', light: 'dark', dark: 'auto' };
const inWorkspace = () => app.connected && !!app.currentWorkspace;
const scope = () => ({ kind: 'workspace', id: app.workspaceId }) as const;

const APK = { id: 'apk-updates', icon: RefreshCw, label: () => at('check'), available: () => __SELF_UPDATE__ && apk.enabled && apk.updates, run: () => apk.check(true) } as const satisfies CommandDef;

const hasProject = () => app.connected && app.projects.length > 0;

export const more = [
  { id: 'review', icon: GitPullRequest, label: label('review.palette'), section: 'views', available: connected, run: () => reviewChanges() },
  { id: 'review-beside', icon: GitPullRequest, label: label('review.beside'), section: 'views', available: connected, run: () => reviewChanges(true) },
  { id: 'browser', icon: Globe, label: label('palette.openBrowser'), section: 'views', run: () => (app.view = 'browser') },
  { id: 'graph', icon: GitGraph, label: label('palette.openGit'), section: 'views', available: hasProject, run: () => app.openProjectTab('graph') },
  { id: 'worktrees', icon: FolderTree, label: label('palette.openWorktrees'), section: 'views', available: hasProject, run: () => app.openProjectTab('worktrees') },
  { id: 'system', icon: Activity, label: () => st('open'), section: 'views', available: connected, run: () => app.open('system') },
  { id: 'usage', icon: Gauge, label: () => ust('open'), section: 'views', available: connected, run: () => app.open('usage') },
  { id: 'prs', icon: GitPullRequestArrow, label: () => gt('open'), section: 'views', available: connected, run: () => openPrReviews() },
  { id: 'review-close', icon: GitPullRequest, label: label('review.close'), available: () => !!beside.key, run: () => closeSideReview() },
  { id: 'agents', icon: LayoutGrid, label: label('palette.openAgents'), run: () => (app.view = 'agents') },
  { id: 'connect', icon: Server, label: () => t(app.connected ? 'palette.switchHost' : 'palette.connectHost'), run: () => app.open('connect') },
  { id: 'add-host', icon: Server, label: label('palette.addHost'), available: () => app.connected || app.hosts.length > 1, run: () => app.open('connect', 'add') },
  { id: 'disconnect', icon: Unplug, label: label('palette.disconnect'), available: connected, run: () => app.disconnect() },
  { id: 'theme', icon: SunMoon, label: label('palette.toggleTheme'), run: () => saveSettings({ theme: nextTheme[settings.theme] }) },
  { id: 'live-review', icon: GitGraph, label: label('palette.toggleLiveReview'), run: () => toggleLiveReview() },
  { id: 'skills', icon: Puzzle, label: label('palette.skills'), available: connected, run: () => app.open('settings', 'skills') },
  { id: 'logs', icon: ScrollText, label: () => dt('show'), run: () => diag.open('live') },
  { id: 'feedback', icon: MessageSquarePlus, label: () => ft('title'), run: () => feedback.show() },
  { id: 'record', icon: Clapperboard, label: () => rt('palette'), available: () => recorder.phase === 'idle', run: () => recorder.setup() },
  { id: 'tour', icon: Compass, label: () => tt('palette'), run: () => replayTour() },
  { id: 'organize', icon: WandSparkles, label: label('org.palette'), available: connected, run: () => organizer.show() },
  { id: 'ws-broadcast', icon: Megaphone, label: label('palette.broadcastWorkspace'), available: inWorkspace, run: () => broadcastTo(scope()) },
  { id: 'ws-hold', icon: Pause, label: label('palette.holdWorkspace'), available: inWorkspace, run: () => holdAll(scope(), 'hold') },
  { id: 'ws-resume', icon: Play, label: label('palette.resumeWorkspace'), available: inWorkspace, run: () => holdAll(scope(), 'resume') },
  { id: 'group-new', icon: Users, label: label('palette.groupFromSelection'), available: () => app.connected && app.multi.size > 0, run: () => createGroup([...app.multi]) },
  { id: 'sel-broadcast', icon: Megaphone, label: label('palette.broadcastSelection'), available: () => app.connected && app.multi.size > 0, run: () => broadcastTo({ kind: 'items', keys: [...app.multi] }) },
  { id: 'ports', icon: EthernetPort, label: label('palette.ports'), available: connected, run: () => app.open('ports') },
  { id: 'forward-port', icon: EthernetPort, label: label('palette.forwardPort'), available: () => app.connected && app.conn.target.kind !== 'local', run: () => app.open('ports', 'forward') },
  { id: 'updates', icon: RefreshCw, label: () => ut('check'), available: () => updater.enabled && !app.mobile, run: () => updater.check(true) },
  { id: 'notifications', icon: Bell, label: label('center.title'), run: () => app.open('notifications') },
  ...(__SELF_UPDATE__ ? [APK] : []),
] as const satisfies readonly CommandDef[];

export type CommandId = CoreId | (typeof more)[number]['id'];

const CORE_ICONS: Record<CoreId, CommandIcon | null> = {
  'kanban': KanbanSquare,
  'panel': PanelRight,
  'source-control': GitGraph,
  'split-right': Columns2,
  'split-down': Rows2,
  'settings': Settings,
  'ws-new': FolderTree,
  'folder-new': FolderPlus,
  'rescan-folders': RefreshCw,
  'shortcuts': Keyboard,
  'new-window': AppWindow,
  'sidebar': PanelLeft,
  'tab-new': Plus,
  'tab-close': X2,
  'tab-reopen': Undo2,
  'palette': Search,
  'reconnect': null,
  'tab-next': null,
  'tab-prev': null,
  'tab-index': null,
  'focus-left': null,
  'focus-right': null,
  'focus-up': null,
  'focus-down': null,
  'term-find': null,
  'term-copy': null,
  'term-paste': null,
  'font-up': null,
  'font-down': null,
  'font-reset': null,
  'close-window': null,
};

const isCore = (id: string): id is CoreId => id in CORE_LABELS;

export function registerMore() {
  register(more);
  for (const c of allCommands()) {
    c.section ??= 'actions';
    if (!isCore(c.id)) continue;
    c.icon = CORE_ICONS[c.id] ?? undefined;
    c.label = label(CORE_LABELS[c.id]);
    if (AGENTS_VIEW.has(c.id)) c.agentsView = true;
  }
}
