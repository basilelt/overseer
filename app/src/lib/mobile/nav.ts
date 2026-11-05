import type { MobileTab } from './layout';

export interface NavState {
  view: 'agents' | 'kanban' | 'browser';
  key: string | null;
}

export function nextTab(current: MobileTab, before: NavState, after: NavState): MobileTab {
  if (after.view !== before.view) {
    if (after.view === 'kanban') return 'tasks';
    if (current === 'tasks') return after.key?.startsWith('review:') ? 'review' : 'agents';
  }
  if (after.key !== before.key && after.key) {
    if (after.key.startsWith('review:')) return 'review';
    if ((after.key.startsWith('line:') || after.key.startsWith('term:')) && (current === 'review' || current === 'more')) return 'agents';
  }
  return current;
}

export function viewForTab(tab: MobileTab, view: NavState['view']): NavState['view'] {
  if (tab === 'tasks') return 'kanban';
  if (tab === 'agents' || tab === 'review') return 'agents';
  return view === 'kanban' ? 'agents' : view;
}
