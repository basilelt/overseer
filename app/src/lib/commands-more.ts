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
import type { Component } from 'svelte';
import { allCommands, connected, label, register, type CommandDef } from './commands';
import { diag, dt } from './diag.svelte';
import { feedback, ft } from './feedback.svelte';
import { toggleLiveReview } from './git/live-actions';
import { openPrReviews } from './github/open.svelte';
import { gt } from './github/strings';
import { t } from './i18n.svelte';
import { apk, at } from './mobile/apk-update.svelte';
import { organizer } from './organize.svelte';
import { recorder, rt } from './record.svelte';
import { beside } from './review/beside.svelte';
import { closeSideReview, openReview, openReviewBeside, reviewTargetFor } from './review/open';
import { saveSettings, settings, type ThemePref } from './settings.svelte';
import { app } from './state.svelte';
import { st } from './system/strings';
import { replayTour } from './tour/replay';
import { tt } from './tour/strings';
import { ust } from './usage/strings';
import { updater, ut } from './updater.svelte';
import { broadcastTo, createGroup, holdAll } from './wsactions';

function reviewChanges(side = false) {
  const target = reviewTargetFor(app.selected);
  if (target) (side ? openReviewBeside : openReview)(target);
  else app.notify(t('review.noLine'), 'error');
}

const nextTheme: Record<ThemePref, ThemePref> = { auto: 'light', light: 'dark', dark: 'auto' };
const inWorkspace = () => app.connected && !!app.currentWorkspace;
const scope = () => ({ kind: 'workspace', id: app.workspaceId }) as const;

const APK: CommandDef = { id: 'apk-updates', label: () => at('check'), available: () => __SELF_UPDATE__ && apk.enabled && apk.updates, run: () => apk.check(true) };

export const more: CommandDef[] = [
  { id: 'review', label: label('review.palette'), section: 'views', available: connected, run: () => reviewChanges() },
  { id: 'review-beside', label: label('review.beside'), section: 'views', available: connected, run: () => reviewChanges(true) },
  { id: 'browser', label: label('palette.openBrowser'), section: 'views', run: () => (app.view = 'browser') },
  { id: 'graph', label: label('palette.openGit'), section: 'views', run: () => app.openProjectTab('graph') },
  { id: 'worktrees', label: label('palette.openWorktrees'), section: 'views', run: () => app.openProjectTab('worktrees') },
  { id: 'system', label: () => st('open'), section: 'views', available: connected, run: () => app.open('system') },
  { id: 'usage', label: () => ust('open'), section: 'views', available: connected, run: () => app.open('usage') },
  { id: 'prs', label: () => gt('open'), section: 'views', available: connected, run: () => openPrReviews() },
  { id: 'review-close', label: label('review.close'), available: () => !!beside.key, run: () => closeSideReview() },
  { id: 'agents', label: label('palette.openAgents'), run: () => (app.view = 'agents') },
  { id: 'connect', label: () => t(app.connected ? 'palette.switchHost' : 'palette.connectHost'), run: () => app.open('connect') },
  { id: 'add-host', label: label('palette.addHost'), available: () => app.connected || app.hosts.length > 1, run: () => app.open('connect', 'add') },
  { id: 'disconnect', label: label('palette.disconnect'), available: connected, run: () => app.disconnect() },
  { id: 'theme', label: label('palette.toggleTheme'), run: () => saveSettings({ theme: nextTheme[settings.theme] }) },
  { id: 'live-review', label: label('palette.toggleLiveReview'), run: () => toggleLiveReview() },
  { id: 'skills', label: label('palette.skills'), available: connected, run: () => app.open('settings', 'skills') },
  { id: 'logs', label: () => dt('show'), run: () => diag.open('live') },
  { id: 'feedback', label: () => ft('title'), run: () => feedback.show() },
  { id: 'record', label: () => rt('palette'), available: () => recorder.phase === 'idle', run: () => recorder.setup() },
  { id: 'tour', label: () => tt('palette'), run: () => replayTour() },
  { id: 'organize', label: label('org.palette'), available: connected, run: () => organizer.show() },
  { id: 'ws-broadcast', label: label('palette.broadcastWorkspace'), available: inWorkspace, run: () => broadcastTo(scope()) },
  { id: 'ws-hold', label: label('palette.holdWorkspace'), available: inWorkspace, run: () => holdAll(scope(), 'hold') },
  { id: 'ws-resume', label: label('palette.resumeWorkspace'), available: inWorkspace, run: () => holdAll(scope(), 'resume') },
  { id: 'group-new', label: label('palette.groupFromSelection'), available: () => app.connected && app.multi.size > 0, run: () => createGroup([...app.multi]) },
  { id: 'sel-broadcast', label: label('palette.broadcastSelection'), available: () => app.connected && app.multi.size > 0, run: () => broadcastTo({ kind: 'items', keys: [...app.multi] }) },
  { id: 'ports', label: label('palette.ports'), available: connected, run: () => app.open('ports') },
  { id: 'forward-port', label: label('palette.forwardPort'), available: () => app.connected && app.conn.target.kind !== 'local', run: () => app.open('ports', 'forward') },
  { id: 'updates', label: () => ut('check'), available: () => updater.enabled && !app.mobile, run: () => updater.check(true) },
  { id: 'notifications', label: label('center.title'), run: () => app.open('notifications') },
  ...(__SELF_UPDATE__ ? [APK] : []),
];

const ICONS: Record<string, Component<{ size?: number }>> = {
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
  'apk-updates': RefreshCw,
  'review': GitPullRequest,
  'review-beside': GitPullRequest,
  'browser': Globe,
  'graph': GitGraph,
  'worktrees': FolderTree,
  'system': Activity,
  'usage': Gauge,
  'prs': GitPullRequestArrow,
  'review-close': GitPullRequest,
  'agents': LayoutGrid,
  'connect': Server,
  'add-host': Server,
  'disconnect': Unplug,
  'theme': SunMoon,
  'live-review': GitGraph,
  'skills': Puzzle,
  'logs': ScrollText,
  'feedback': MessageSquarePlus,
  'record': Clapperboard,
  'tour': Compass,
  'organize': WandSparkles,
  'ws-broadcast': Megaphone,
  'ws-hold': Pause,
  'ws-resume': Play,
  'group-new': Users,
  'sel-broadcast': Megaphone,
  'ports': EthernetPort,
  'forward-port': EthernetPort,
  'updates': RefreshCw,
  'notifications': Bell,
};

export function registerMore() {
  register(more);
  for (const c of allCommands()) c.icon = ICONS[c.id];
}
