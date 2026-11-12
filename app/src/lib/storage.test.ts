import { beforeEach, describe, expect, it, vi } from 'vitest';
import { KEY_SETTINGS, KEY_KNOWN_TABS, KEY_SESSION_PAGES, KEY_LIVE_REVIEW, KEY_SWITCHBOARD_IMPORT, KEY_NOTIFY_ASKED, KEY_NOTIFIED, KEY_NOTIFICATIONS_PREFIX, KEY_RENDERER, KEY_REVIEW_MODE, KEY_REVIEW_AUTO_REFRESH, KEY_REVIEW_VIEW, KEY_REVIEW_BASE, KEY_CHAT_MODES, KEY_FEEDBACK_REPORTED, KEY_DRAFTS_CHAT, KEY_DRAFTS_REVIEW, KEY_DRAFTS_TASK, KEY_DEMO, KEY_DEMO_PREFIX, KEY_MOCK_IMPORTED, localStore, readItem, readJson, removeItem, writeItem, writeJson } from './storage';

function memoryStorage() {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  };
}

beforeEach(() => vi.stubGlobal('localStorage', memoryStorage()));
const broken = {
  getItem: () => {
    throw new Error('denied');
  },
  setItem: () => {
    throw new Error('full');
  },
  removeItem: () => {
    throw new Error('denied');
  },
};

describe('storage helper', () => {
  it('keeps every persisted key string, legacy sb. ones included', () => {
    expect({
      settings: KEY_SETTINGS,
      knownTabs: KEY_KNOWN_TABS,
      sessionPages: KEY_SESSION_PAGES,
      liveReview: KEY_LIVE_REVIEW,
      switchboardImport: KEY_SWITCHBOARD_IMPORT,
      notifyAsked: KEY_NOTIFY_ASKED,
      notified: KEY_NOTIFIED,
      notificationsPrefix: KEY_NOTIFICATIONS_PREFIX,
      renderer: KEY_RENDERER,
      reviewMode: KEY_REVIEW_MODE,
      reviewAutoRefresh: KEY_REVIEW_AUTO_REFRESH,
      reviewView: KEY_REVIEW_VIEW,
      reviewBase: KEY_REVIEW_BASE,
      chatModes: KEY_CHAT_MODES,
      feedbackReported: KEY_FEEDBACK_REPORTED,
      draftsChat: KEY_DRAFTS_CHAT,
      draftsReview: KEY_DRAFTS_REVIEW,
      draftsTask: KEY_DRAFTS_TASK,
      demo: KEY_DEMO,
      demoPrefix: KEY_DEMO_PREFIX,
      mockImported: KEY_MOCK_IMPORTED,
    }).toEqual({
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
    });
  });

  it('round-trips strings and removes them', () => {
    expect(readItem('k')).toBeNull();
    expect(writeItem('k', 'v')).toBe(true);
    expect(readItem('k')).toBe('v');
    expect(removeItem('k')).toBe(true);
    expect(readItem('k')).toBeNull();
  });

  it('round-trips JSON and reads null for missing or invalid values', () => {
    expect(writeJson('j', { a: [1] })).toBe(true);
    expect(readJson('j')).toEqual({ a: [1] });
    expect(readJson('missing')).toBeNull();
    writeItem('bad', '{nope');
    expect(readJson('bad')).toBeNull();
  });

  it('does not throw when storage throws', () => {
    vi.stubGlobal('localStorage', broken);
    expect(readItem('k')).toBeNull();
    expect(writeItem('k', 'v')).toBe(false);
    expect(removeItem('k')).toBe(false);
    expect(readJson('k')).toBeNull();
    expect(writeJson('k', 1)).toBe(false);
  });

  it('does not throw without storage', () => {
    vi.stubGlobal('localStorage', undefined);
    expect(readItem('k')).toBeNull();
    expect(writeItem('k', 'v')).toBe(false);
    expect(removeItem('k')).toBe(false);
  });

  it('does not throw when the value cannot be serialised', () => {
    const loop: Record<string, unknown> = {};
    loop.self = loop;
    expect(writeJson('k', loop)).toBe(false);
  });

  it('does not throw when reading the storage accessor throws', () => {
    const saved = Object.getOwnPropertyDescriptor(globalThis, 'localStorage')!;
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      get() {
        throw new Error('blocked');
      },
    });
    try {
      expect(readItem('k')).toBeNull();
      expect(writeItem('k', 'v')).toBe(false);
    } finally {
      Object.defineProperty(globalThis, 'localStorage', saved);
    }
  });
});

describe('localStore', () => {
  it('returns the browser storage', () => {
    expect(localStore()).toBe(globalThis.localStorage);
  });

  it('is undefined when storage is missing', () => {
    vi.stubGlobal('localStorage', undefined);
    expect(localStore()).toBeUndefined();
  });

  it('is undefined when reading the storage throws', () => {
    const saved = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
    Object.defineProperty(globalThis, 'localStorage', { get: () => { throw new Error('denied'); }, configurable: true });
    try {
      expect(localStore()).toBeUndefined();
    } finally {
      if (saved) Object.defineProperty(globalThis, 'localStorage', saved);
      else Reflect.deleteProperty(globalThis, 'localStorage');
    }
  });
});
