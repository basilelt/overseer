export const KEY_SETTINGS = 'overseer.settings';
export const KEY_KNOWN_TABS = 'overseer.knownTabs';
export const KEY_SESSION_PAGES = 'overseer.sessionPages';
export const KEY_LIVE_REVIEW = 'overseer.liveReview';
export const KEY_SWITCHBOARD_IMPORT = 'overseer.switchboardImport';
export const KEY_NOTIFY_ASKED = 'overseer.notifyAsked';
export const KEY_NOTIFIED = 'overseer.notified';
export const KEY_NOTIFICATIONS_PREFIX = 'overseer.notifications.';
export const KEY_RENDERER = 'overseer.renderer';
export const KEY_REVIEW_MODE = 'overseer.review.mode';
export const KEY_REVIEW_AUTO_REFRESH = 'overseer.review.autoRefresh';
export const KEY_REVIEW_VIEW = 'overseer.review.view';
export const KEY_REVIEW_BASE = 'overseer.review.base';
export const KEY_CHAT_MODES = 'sb.chatModes';
export const KEY_FEEDBACK_REPORTED = 'sb.feedback.reported';
export const KEY_DRAFTS_CHAT = 'overseer.drafts.chat';
export const KEY_DRAFTS_REVIEW = 'overseer.drafts.review';
export const KEY_DRAFTS_TASK = 'overseer.drafts.task';
export const KEY_DEMO = 'overseer.demo';
export const KEY_DEMO_PREFIX = 'overseer.demo:';
export const KEY_MOCK_IMPORTED = 'overseer.mockImported';

export function localStore(): Storage | undefined {
  try {
    return globalThis.localStorage ?? undefined;
  } catch {
    return undefined;
  }
}

export function readItem(key: string): string | null {
  try {
    return localStore()?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

export function writeItem(key: string, value: string): boolean {
  try {
    const s = localStore();
    if (!s) return false;
    s.setItem(key, value);
    return true;
  } catch {
    return false;
  }
}

export function removeItem(key: string): boolean {
  try {
    const s = localStore();
    if (!s) return false;
    s.removeItem(key);
    return true;
  } catch {
    return false;
  }
}

export function readJson(key: string): unknown {
  const raw = readItem(key);
  if (!raw) return null;
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

export function writeJson(key: string, value: unknown): boolean {
  try {
    return writeItem(key, JSON.stringify(value));
  } catch {
    return false;
  }
}
