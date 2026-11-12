import { gt } from './github/strings';
import { t } from './i18n.svelte';
import { reviewTitle } from './review/open';
import { app, projectName } from './state.svelte';
import { st } from './system/strings';
import type { TabInfo } from './tabs';
import { ust } from './usage/strings';

export function tabLabel(tab: TabInfo, opts: { browserHost: boolean }): string {
  if (tab.item) return tab.item.title;
  if (tab.kind === 'browser') {
    if (!opts.browserHost) return t('tabs.browser');
    try {
      return tab.arg ? new URL(tab.arg).host || tab.arg : t('tabs.browser');
    } catch {
      return tab.arg || t('tabs.browser');
    }
  }
  if (tab.kind === 'review') return t('tabs.review', { title: reviewTitle(tab.key) ?? tab.arg });
  if (tab.kind === 'graph') return t('tabs.graphOf', { name: projectName(app.projects, tab.arg) });
  if (tab.kind === 'worktrees') return t('tabs.worktreesOf', { name: projectName(app.projects, tab.arg) });
  if (tab.kind === 'system') return st('title');
  if (tab.kind === 'usage') return ust('title');
  if (tab.kind === 'prs') return gt('title');
  return tab.key;
}
