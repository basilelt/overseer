import { gt } from './github/strings';
import { t } from './i18n.svelte';
import { reviewTitle } from './review/open';
import { app, projectName } from './state.svelte';
import { st } from './system/strings';
import type { TabInfo } from './tabs';
import { ust } from './usage/strings';

function browserLabel(arg: string, host: boolean): string {
  if (!host) return t('tabs.browser');
  try {
    return arg ? new URL(arg).host || arg : t('tabs.browser');
  } catch {
    return arg || t('tabs.browser');
  }
}

export function tabLabel(tab: TabInfo, opts: { browserHost: boolean }): string {
  if (tab.item) return tab.item.title;
  if (tab.kind === 'browser') return browserLabel(tab.arg, opts.browserHost);
  if (tab.kind === 'review') return t('tabs.review', { title: reviewTitle(tab.key) ?? tab.arg });
  if (tab.kind === 'graph') return t('tabs.graphOf', { name: projectName(app.projects, tab.arg) });
  if (tab.kind === 'worktrees') return t('tabs.worktreesOf', { name: projectName(app.projects, tab.arg) });
  if (tab.kind === 'system') return st('title');
  if (tab.kind === 'usage') return ust('title');
  if (tab.kind === 'prs') return gt('title');
  return tab.key;
}
