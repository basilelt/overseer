import { describe, expect, it } from 'vitest';
import { en } from './i18n/en';
import { dict as fr } from './i18n/fr';
import type { Key } from './i18n.svelte';
import {
  availableCategories,
  availableSections,
  categoryCounts,
  CATEGORIES,
  ENTRIES,
  escapeAction,
  initialCategory,
  isHit,
  matches,
  normalize,
  phonePage,
  pickCategory,
  search,
  SECTIONS,
  sectionsOf,
  showEntry,
  showSection,
  type Gates,
  type Text,
} from './settings-index';

const all: Gates = { desktop: true, nonMac: true, connected: true, profiles: true, hooks: true, updater: true, apk: true, switchboard: true };
const none: Gates = { desktop: false, nonMac: false, connected: false, profiles: false, hooks: false, updater: false, apk: false, switchboard: false };
const en2: Text = (k) => [en[k]];
const both: Text = (k) => [(fr as Partial<Record<Key, string>>)[k] ?? en[k], en[k]];

describe('index data', () => {
  it('only references existing i18n keys', () => {
    const keys = [...CATEGORIES.map((c) => c.title), ...SECTIONS.map((s) => s.title), ...ENTRIES.flatMap((e) => e.keys)];
    expect(keys.filter((k) => !(k in en))).toEqual([]);
  });

  it('gives every entry a known section and every section a known category', () => {
    const sections = new Set(SECTIONS.map((s) => s.id));
    const cats = new Set(CATEGORIES.map((c) => c.id));
    expect(ENTRIES.every((e) => sections.has(e.section))).toBe(true);
    expect(SECTIONS.every((s) => cats.has(s.category))).toBe(true);
    expect(new Set(SECTIONS.map((s) => s.id)).size).toBe(SECTIONS.length);
  });

  it('has unique entry ids per section', () => {
    const ids = ENTRIES.map((e) => `${e.section}/${e.id}`);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe('normalize and matches', () => {
  it('folds case, accents and spaces', () => {
    expect(normalize('  Général  THÈME ')).toBe('general theme');
  });

  it('matches every token anywhere in the texts', () => {
    expect(matches('font size', ['Terminal font size'])).toBe(true);
    expect(matches('size font', ['Terminal font size'])).toBe(true);
    expect(matches('font color', ['Terminal font size'])).toBe(false);
  });

  it('matches accent-insensitively and treats blank as match-all', () => {
    expect(matches('reglage', ['Réglages'])).toBe(true);
    expect(matches('   ', ['x'])).toBe(true);
  });
});

describe('categories and sections', () => {
  it('lists every category when everything is available', () => {
    expect(availableCategories(all).map((c) => c.id)).toEqual(CATEGORIES.map((c) => c.id));
  });

  it('drops gated sections and categories left empty', () => {
    const ids = availableCategories(none).map((c) => c.id);
    expect(ids).not.toContain('updates');
    expect(ids).toContain('general');
    expect(ids).toContain('help');
    expect(availableSections(none).map((s) => s.id)).not.toContain('keyboard');
  });

  it('returns the sections of a category in order', () => {
    expect(sectionsOf('terminal', all).map((s) => s.id)).toEqual(['terminal', 'keyboard']);
    expect(sectionsOf('terminal', none).map((s) => s.id)).toEqual(['terminal']);
  });

  it('keeps a valid category and falls back to the first otherwise', () => {
    expect(pickCategory('agents', all)).toBe('agents');
    expect(pickCategory('updates', none)).toBe('general');
    expect(pickCategory(null, all)).toBe('general');
  });

  it('opens notifications for the notifications argument', () => {
    expect(initialCategory('notifications')).toBe('notifications');
    expect(initialCategory(null)).toBe('general');
    expect(initialCategory('other')).toBe('general');
  });
});

describe('search', () => {
  it('returns nothing for a blank query', () => {
    expect(search('  ', all, en2)).toEqual({ hits: {}, categories: [], count: 0 });
  });

  it('finds a setting by label and limits the section to matching rows', () => {
    const r = search('scrollback', all, en2);
    expect(r.hits.terminal).toEqual(['scrollback']);
    expect(r.categories).toEqual(['terminal']);
    expect(r.count).toBe(1);
  });

  it('finds the GPU rendering toggle by its label and its hint', () => {
    expect(search('gpu', all, en2).hits.terminal).toEqual(['gpuTerminal']);
    expect(search('webgl', all, en2).hits.terminal).toEqual(['gpuTerminal']);
  });

  it('finds the privacy policy link under help, even without a connection', () => {
    const r = search('privacy', none, en2);
    expect(r.hits.feedback).toEqual(['privacy']);
    expect(r.categories).toEqual(['help']);
  });

  it('finds a setting by its hint text', () => {
    const r = search('zsh', all, en2);
    expect(r.hits.terminal).toEqual(['shiftEnter']);
  });

  it('shows the whole section when its title matches', () => {
    const r = search('appearance', all, en2);
    expect(r.hits.appearance).toBe('all');
    expect(r.count).toBe(ENTRIES.filter((e) => e.section === 'appearance').length);
  });

  it('matches across categories', () => {
    const r = search('check for updates', all, en2);
    expect(r.categories).toEqual(['updates']);
    const w = search('notif', all, en2);
    expect(w.categories).toContain('notifications');
  });

  it('limits the notifications section to the matching rows', () => {
    expect(search('quiet hours', all, en2).hits.notifications).toEqual(['notifyQuiet']);
    expect(search('minimum gap', all, en2).hits.notifications).toEqual(['notifyBehaviour']);
    expect(search('needs input', all, en2).hits.notifications).toEqual(['notifyEvents']);
    expect(search('send test', all, en2).hits.notifications).toEqual(['notifyPermission']);
    expect(search('mute', all, en2).hits.notifications).toEqual(['notifyMutes']);
  });

  it('limits the server section to the matching rows', () => {
    const r = search('restore agent', all, en2);
    expect(r.hits.server).toEqual(['afterReboot']);
    expect(showEntry(r, 'server', 'keepRunning')).toBe(false);
    expect(isHit(r, 'server', 'afterReboot')).toBe(true);
    expect(search('keep running', all, en2).hits.server).toEqual(['keepRunning']);
  });

  it('limits the updates sections to the matching rows', () => {
    expect(search('check for updates', all, en2).hits.updater).toEqual(['update']);
    expect(search('github token', all, en2).hits.updater).toBeUndefined();
    expect(search('version', all, en2).hits.apk).toEqual(['apk']);
  });

  it('matches the notification kinds in French with accents folded', () => {
    expect(search('echoue', all, both).hits.notifications).toEqual(['notifyEvents']);
  });

  it('has no row for the server section hidden when not connected', () => {
    expect(search('reboot', { ...all, connected: false }, en2).hits.server).toBeUndefined();
  });

  it('matches French strings and English strings alike', () => {
    expect(search('historique', all, both).hits.terminal).toContain('scrollback');
    expect(search('scrollback', all, both).hits.terminal).toContain('scrollback');
  });

  it('ignores sections and entries the gates hide', () => {
    expect(search('ctrl+w', { ...all, nonMac: false }, en2).hits.keyboard).toBeUndefined();
    expect(search('ctrl+w', all, en2).hits.keyboard).toEqual(['ctrlW']);
    expect(search('keep server', none, en2).count).toBe(0);
  });

  it('reports no hits for unknown text', () => {
    const r = search('zzzzzz', all, en2);
    expect(r.count).toBe(0);
    expect(r.categories).toEqual([]);
  });

  it('counts per category', () => {
    const r = search('size', all, en2);
    const c = categoryCounts(r);
    expect(c.terminal).toBeGreaterThan(0);
    expect(c.help).toBe(0);
  });
});

describe('visibility helpers', () => {
  const r = search('scrollback', all, en2);

  it('shows everything without a search', () => {
    expect(showSection(null, 'terminal')).toBe(true);
    expect(showEntry(null, 'terminal', 'fontSize')).toBe(true);
    expect(isHit(null, 'terminal', 'fontSize')).toBe(false);
  });

  it('shows only matching sections and rows during a search', () => {
    expect(showSection(r, 'terminal')).toBe(true);
    expect(showSection(r, 'links')).toBe(false);
    expect(showEntry(r, 'terminal', 'scrollback')).toBe(true);
    expect(showEntry(r, 'terminal', 'fontSize')).toBe(false);
    expect(showEntry(r, 'links', 'linkAsk')).toBe(false);
  });

  it('highlights matching rows but not rows shown because the title matched', () => {
    expect(isHit(r, 'terminal', 'scrollback')).toBe(true);
    const t = search('terminal', all, en2);
    expect(showEntry(t, 'terminal', 'fontSize')).toBe(true);
    expect(isHit(t, 'terminal', 'fontSize')).toBe(false);
  });
});

describe('phone navigation', () => {
  it('shows the list, a section, or results', () => {
    expect(phonePage('', null)).toBe('list');
    expect(phonePage('', 'agents')).toBe('section');
    expect(phonePage('font', 'agents')).toBe('results');
    expect(phonePage('font', null)).toBe('results');
  });
});

describe('escape', () => {
  it('clears a search first', () => {
    expect(escapeAction('font', false, 'general')).toBe('clear');
    expect(escapeAction('font', true, 'general')).toBe('clear');
  });

  it('goes back from a phone section, otherwise closes', () => {
    expect(escapeAction('', true, 'general')).toBe('back');
    expect(escapeAction('', true, null)).toBe('close');
    expect(escapeAction('', false, 'general')).toBe('close');
  });
});

it('finds the Graphite palette and its color controls in settings search', () => {
  const hits = search('graphite', all, en2);
  expect(hits.hits.appearance).toEqual(['palettes']);
});

it('finds Solarized Soft and its color controls in settings search', () => {
  expect(search('solarized soft', all, en2).hits.appearance).toEqual(['palettes']);
});
