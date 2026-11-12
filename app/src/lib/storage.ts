export const KEYS = {
  settings: 'overseer.settings',
  knownTabs: 'overseer.knownTabs',
  sessionPages: 'overseer.sessionPages',
  liveReview: 'overseer.liveReview',
  switchboardImport: 'overseer.switchboardImport',
  notifyAsked: 'overseer.notifyAsked',
  notified: 'overseer.notified',
  notificationsPrefix: 'overseer.notifications.',
  renderer: 'overseer.renderer',
  reviewMode: 'overseer.review.mode',
  reviewAutoRefresh: 'overseer.review.autoRefresh',
  reviewView: 'overseer.review.view',
  reviewBase: 'overseer.review.base',
  chatModes: 'sb.chatModes',
  feedbackReported: 'sb.feedback.reported',
  draftsChat: 'overseer.drafts.chat',
  draftsReview: 'overseer.drafts.review',
  draftsTask: 'overseer.drafts.task',
  demo: 'overseer.demo',
  demoPrefix: 'overseer.demo:',
  mockImported: 'overseer.mockImported',
} as const;

function store(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

export function readItem(key: string): string | null {
  try {
    return store()?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

export function writeItem(key: string, value: string): boolean {
  try {
    const s = store();
    if (!s) return false;
    s.setItem(key, value);
    return true;
  } catch {
    return false;
  }
}

export function removeItem(key: string): boolean {
  try {
    const s = store();
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
