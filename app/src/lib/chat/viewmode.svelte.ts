import { isPhone } from '../mobile/ui.svelte';
import { tabMode, withMode, type ModeItem, type ViewMode } from './viewmode';
import { readJson, writeJson, KEY_CHAT_MODES } from '../storage';

const STORE = KEY_CHAT_MODES;

function load(): Record<string, ViewMode> {
  const v = readJson(STORE);
  return v && typeof v === 'object' ? (v as Record<string, ViewMode>) : {};
}

const state = $state<{ saved: Record<string, ViewMode> }>({ saved: load() });

export function modeOf(item: (ModeItem & { key?: string }) | undefined): ViewMode {
  return tabMode(state.saved, isPhone() ? 'phone' : 'desktop', item);
}

export function setMode(item: ModeItem & { key?: string }, mode: ViewMode) {
  state.saved = withMode(state.saved, isPhone() ? 'phone' : 'desktop', item, mode);
  writeJson(STORE, state.saved);
}
