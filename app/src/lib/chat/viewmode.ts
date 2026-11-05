export type ViewMode = 'chat' | 'terminal';
export type LayoutKind = 'phone' | 'desktop';

export const CHAT_AGENTS = ['claude', 'codex', 'gemini'];

export interface ModeItem {
  kind: string;
  lineId?: string;
}

export function chatCapable(item: ModeItem | undefined): boolean {
  return !!item?.lineId && CHAT_AGENTS.includes(item.kind);
}

export function defaultMode(layout: LayoutKind, item: ModeItem | undefined): ViewMode {
  return layout === 'phone' && chatCapable(item) ? 'chat' : 'terminal';
}

export function modeKey(layout: LayoutKind, item: ModeItem & { key?: string }): string {
  return `${layout}:${item.lineId ? `line:${item.lineId}` : (item.key ?? '')}`;
}

export function tabMode(saved: Record<string, ViewMode>, layout: LayoutKind, item: (ModeItem & { key?: string }) | undefined): ViewMode {
  if (!item || !chatCapable(item)) return 'terminal';
  return saved[modeKey(layout, item)] ?? defaultMode(layout, item);
}

export function withMode(saved: Record<string, ViewMode>, layout: LayoutKind, item: ModeItem & { key?: string }, mode: ViewMode): Record<string, ViewMode> {
  const k = modeKey(layout, item);
  const next = { ...saved };
  if (mode === defaultMode(layout, item)) delete next[k];
  else next[k] = mode;
  const keys = Object.keys(next);
  for (const old of keys.slice(0, Math.max(0, keys.length - 200))) delete next[old];
  return next;
}
