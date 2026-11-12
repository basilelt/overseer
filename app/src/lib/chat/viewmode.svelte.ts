import { isPhone } from '../mobile/ui.svelte';
import { tabMode, withMode, type ModeItem, type ViewMode } from './viewmode';

const STORE = 'sb.chatModes';

function load(): Record<string, ViewMode> {
  try {
    const v = JSON.parse(globalThis.localStorage?.getItem(STORE) ?? '{}');
    return v && typeof v === 'object' ? v : {};
  } catch {
    return {};
  }
}

const state = $state<{ saved: Record<string, ViewMode> }>({ saved: load() });

export function modeOf(item: (ModeItem & { key?: string }) | undefined): ViewMode {
  return tabMode(state.saved, isPhone() ? 'phone' : 'desktop', item);
}

export function setMode(item: ModeItem & { key?: string }, mode: ViewMode) {
  state.saved = withMode(state.saved, isPhone() ? 'phone' : 'desktop', item, mode);
  try {
    globalThis.localStorage?.setItem(STORE, JSON.stringify(state.saved));
  } catch {}
}
