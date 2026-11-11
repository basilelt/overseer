import type { Key } from './i18n.svelte';

export type CategoryId = 'general' | 'terminal' | 'agents' | 'usage' | 'notifications' | 'connection' | 'updates' | 'help';
export type SectionId = 'appearance' | 'links' | 'terminal' | 'keyboard' | 'defaults' | 'profiles' | 'hooks' | 'skills' | 'usage' | 'usageCodex' | 'usageGemini' | 'notifications' | 'hosts' | 'server' | 'updater' | 'apk' | 'tour' | 'diag' | 'feedback' | 'switchboard';
export type Gate = 'desktop' | 'nonMac' | 'connected' | 'localServer' | 'profiles' | 'hooks' | 'updater' | 'apk' | 'switchboard';
export type Gates = Record<Gate, boolean>;

export interface Category {
  id: CategoryId;
  title: Key;
}

export interface Section {
  id: SectionId;
  category: CategoryId;
  title: Key;
  gate?: Gate;
}

export interface Entry {
  id: string;
  section: SectionId;
  keys: Key[];
  gate?: Gate;
}

export const CATEGORIES: Category[] = [
  { id: 'general', title: 'settings.cat.general' },
  { id: 'terminal', title: 'settings.cat.terminal' },
  { id: 'agents', title: 'settings.cat.agents' },
  { id: 'usage', title: 'settings.usage' },
  { id: 'notifications', title: 'settings.cat.notifications' },
  { id: 'connection', title: 'settings.cat.connection' },
  { id: 'updates', title: 'settings.cat.updates' },
  { id: 'help', title: 'settings.cat.help' },
];

export const SECTIONS: Section[] = [
  { id: 'appearance', category: 'general', title: 'settings.appearance' },
  { id: 'links', category: 'general', title: 'settings.links' },
  { id: 'terminal', category: 'terminal', title: 'settings.terminal' },
  { id: 'keyboard', category: 'terminal', title: 'settings.keyboard', gate: 'desktop' },
  { id: 'defaults', category: 'agents', title: 'settings.defaults' },
  { id: 'profiles', category: 'agents', title: 'profile.title', gate: 'profiles' },
  { id: 'hooks', category: 'agents', title: 'hooks.title', gate: 'hooks' },
  { id: 'skills', category: 'agents', title: 'skills.title', gate: 'connected' },
  { id: 'usage', category: 'usage', title: 'settings.usage.claude' },
  { id: 'usageCodex', category: 'usage', title: 'settings.usage.codex' },
  { id: 'usageGemini', category: 'usage', title: 'settings.usage.gemini' },
  { id: 'notifications', category: 'notifications', title: 'settings.notifications' },
  { id: 'hosts', category: 'connection', title: 'settings.hosts' },
  { id: 'server', category: 'connection', title: 'settings.server', gate: 'connected' },
  { id: 'updater', category: 'updates', title: 'updater.section', gate: 'updater' },
  { id: 'apk', category: 'updates', title: 'apk.section', gate: 'apk' },
  { id: 'tour', category: 'help', title: 'tour.section' },
  { id: 'diag', category: 'help', title: 'diag.section' },
  { id: 'switchboard', category: 'help', title: 'switchboard.section', gate: 'switchboard' },
  { id: 'feedback', category: 'help', title: 'feedback.section' },
];

export const ENTRIES: Entry[] = [
  { id: 'theme', section: 'appearance', keys: ['settings.theme', 'settings.theme.system', 'settings.theme.light', 'settings.theme.dark'] },
  { id: 'language', section: 'appearance', keys: ['settings.language'] },
  { id: 'palettes', section: 'appearance', keys: ['settings.palette.light', 'settings.palette.dark', 'settings.palette.default.light', 'settings.palette.default.dark', 'settings.theme.graphite', 'settings.theme.solarized', 'settings.palette.new', 'settings.palette.custom', 'settings.color.accent', 'settings.color.bg', 'settings.color.surface', 'settings.color.border', 'settings.color.text', 'settings.color.text-2'] },
  { id: 'linkTarget', section: 'links', keys: ['settings.linkTarget', 'settings.linkTarget.overseer', 'settings.linkTarget.default'] },
  { id: 'linkAsk', section: 'links', keys: ['settings.linkAsk', 'settings.linkAskHint'] },
  { id: 'fontFamily', section: 'terminal', keys: ['settings.fontFamily', 'settings.fontFamilyHint'] },
  { id: 'fontSize', section: 'terminal', keys: ['settings.fontSize'] },
  { id: 'lineHeight', section: 'terminal', keys: ['settings.lineHeight'] },
  { id: 'scrollback', section: 'terminal', keys: ['settings.scrollback', 'settings.scrollbackHint'] },
  { id: 'gpuTerminal', section: 'terminal', keys: ['settings.gpuTerminal', 'settings.gpuTerminalHint'] },
  { id: 'shiftEnter', section: 'terminal', keys: ['settings.shiftEnterNewline', 'settings.shiftEnterNewlineHint'] },
  { id: 'ctrlW', section: 'keyboard', gate: 'nonMac', keys: ['settings.ctrlWClosesTab', 'settings.ctrlWClosesTabHint'] },
  { id: 'newTabShell', section: 'keyboard', keys: ['settings.newTabShell', 'settings.newTabShellHint'] },
  { id: 'wrapTabs', section: 'keyboard', keys: ['settings.wrapTabs', 'settings.wrapTabsHint'] },
  { id: 'defaultAgent', section: 'defaults', keys: ['settings.defaultAgent'] },
  { id: 'autoApprove', section: 'defaults', keys: ['settings.autoApprove', 'settings.autoApproveHint', 'settings.autoApproveAll'] },
  { id: 'defaultModel', section: 'defaults', keys: ['settings.defaultModel'] },
  { id: 'defaultEffort', section: 'defaults', keys: ['settings.defaultEffort'] },
  { id: 'autoConcurrency', section: 'defaults', keys: ['settings.autoConcurrency', 'settings.autoConcurrencyHint'] },
  { id: 'autoOrganize', section: 'defaults', keys: ['settings.autoOrganize', 'settings.autoOrganizeHint'] },
  { id: 'profileDefault', section: 'profiles', keys: ['profile.globalDefault', 'profile.globalHint'] },
  { id: 'profileEdit', section: 'profiles', keys: ['profile.editTitle', 'profile.editHint'] },
  { id: 'hooks', section: 'hooks', keys: ['hooks.hint'] },
  { id: 'skills', section: 'skills', keys: ['skills.hint', 'skills.rtk', 'skills.caveman', 'skills.ponytail', 'palette.skills'] },
  { id: 'claudeLimits', section: 'usage', keys: ['settings.fetchClaudeLimits', 'settings.fetchClaudeLimitsHint'] },
  { id: 'statusline', section: 'usage', keys: ['settings.statusline', 'settings.statuslineHint'] },
  { id: 'codexLimits', section: 'usageCodex', keys: ['settings.codexLimits', 'settings.codexLimitsHint'] },
  { id: 'geminiTokens', section: 'usageGemini', keys: ['settings.geminiTokens', 'settings.geminiTokensHint'] },
  { id: 'notifyPermission', section: 'notifications', keys: ['nb.permission', 'nb.request', 'nb.test'] },
  { id: 'notifyEvents', section: 'notifications', keys: ['nb.events', 'nb.kind.needsInput', 'nb.kind.finished', 'nb.kind.failed', 'nb.kind.allIdle', 'nb.kind.ports', 'nb.kind.updates', 'nb.kind.uploads', 'nb.kind.errors'] },
  { id: 'notifyBehaviour', section: 'notifications', keys: ['nb.behaviour', 'nb.sound', 'nb.preview', 'nb.previewHint', 'nb.unfocused', 'nb.unfocusedHint', 'nb.skipWatched', 'settings.notifyBackgroundToasts', 'nb.cooldown'] },
  { id: 'notifyQuiet', section: 'notifications', keys: ['nb.quiet', 'nb.quietHint', 'nb.from', 'nb.to', 'nb.days'] },
  { id: 'notifyMutes', section: 'notifications', keys: ['nb.mutes', 'nb.mutesHint', 'nb.mutedSessions'] },
  { id: 'hosts', section: 'hosts', keys: ['settings.removeHost'] },
  { id: 'serverBinDir', section: 'hosts', keys: ['settings.serverBinDir', 'settings.serverBinDirHint'] },
  { id: 'keepRunning', section: 'server', keys: ['settings.keepRunning', 'settings.keepRunningHint'] },
  { id: 'afterReboot', section: 'server', keys: ['settings.afterReboot', 'settings.afterRebootHint', 'settings.afterReboot.agents', 'settings.afterReboot.shells', 'settings.afterReboot.nothing'] },
  { id: 'lidClosed', section: 'server', gate: 'localServer', keys: ['settings.lidClosed', 'settings.lidClosedHint', 'settings.lidClosedBattery'] },
  { id: 'uninstall', section: 'server', keys: ['settings.uninstall', 'settings.uninstallRemote', 'settings.uninstallHint', 'settings.uninstallButton'] },
  { id: 'update', section: 'updater', keys: ['updater.version', 'updater.check'] },
  { id: 'apk', section: 'apk', keys: ['apk.version', 'apk.check'] },
  { id: 'tour', section: 'tour', keys: ['tour.again', 'tour.againHint', 'tour.take'] },
  { id: 'organize', section: 'tour', keys: ['org.title', 'org.helpHint', 'org.nudgeAction'] },
  { id: 'diag', section: 'diag', keys: ['diag.show', 'diag.sectionHint'] },
  { id: 'switchboard', section: 'switchboard', keys: ['switchboard.title', 'switchboard.settingsHint', 'switchboard.run'] },
  { id: 'feedback', section: 'feedback', keys: ['feedback.title', 'feedback.sectionHint', 'feedback.repo', 'feedback.repoHint'] },
  { id: 'privacy', section: 'feedback', keys: ['feedback.privacy', 'feedback.privacyHint'] },
];

export type Text = (key: Key) => string[];

export type Hits = Partial<Record<SectionId, 'all' | string[]>>;

export interface SearchResult {
  hits: Hits;
  categories: CategoryId[];
  count: number;
}

export const normalize = (s: string): string =>
  s
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .toLowerCase()
    .replace(/\s+/g, ' ')
    .trim();

export function matches(query: string, texts: string[]): boolean {
  const tokens = normalize(query).split(' ').filter(Boolean);
  if (!tokens.length) return true;
  const hay = normalize(texts.join(' '));
  return tokens.every((tok) => hay.includes(tok));
}

const open = (gate: Gate | undefined, gates: Gates) => !gate || gates[gate];

export function availableSections(gates: Gates, sections: Section[] = SECTIONS): Section[] {
  return sections.filter((s) => open(s.gate, gates));
}

export function availableCategories(gates: Gates, categories: Category[] = CATEGORIES, sections: Section[] = SECTIONS): Category[] {
  const live = new Set(availableSections(gates, sections).map((s) => s.category));
  return categories.filter((c) => live.has(c.id));
}

export function sectionsOf(category: CategoryId, gates: Gates, sections: Section[] = SECTIONS): Section[] {
  return availableSections(gates, sections).filter((s) => s.category === category);
}

export function search(query: string, gates: Gates, text: Text, sections: Section[] = SECTIONS, entries: Entry[] = ENTRIES): SearchResult {
  const hits: Hits = {};
  let count = 0;
  if (!normalize(query)) return { hits, categories: [], count };
  for (const s of availableSections(gates, sections)) {
    if (matches(query, text(s.title))) {
      hits[s.id] = 'all';
      count += entries.filter((e) => e.section === s.id && open(e.gate, gates)).length;
      continue;
    }
    const ids = entries.filter((e) => e.section === s.id && open(e.gate, gates) && matches(query, e.keys.flatMap(text))).map((e) => e.id);
    if (ids.length) {
      hits[s.id] = ids;
      count += ids.length;
    }
  }
  const inHits = new Set(sections.filter((s) => hits[s.id]).map((s) => s.category));
  return { hits, categories: CATEGORIES.filter((c) => inHits.has(c.id)).map((c) => c.id), count };
}

export function showSection(result: SearchResult | null, id: SectionId): boolean {
  return !result || !!result.hits[id];
}

export function showEntry(result: SearchResult | null, section: SectionId, entry: string): boolean {
  if (!result) return true;
  const h = result.hits[section];
  return h === 'all' || (Array.isArray(h) && h.includes(entry));
}

export function isHit(result: SearchResult | null, section: SectionId, entry: string): boolean {
  return !!result && Array.isArray(result.hits[section]) && (result.hits[section] as string[]).includes(entry);
}

export function pickCategory(current: CategoryId | null, gates: Gates): CategoryId {
  const list = availableCategories(gates);
  return list.find((c) => c.id === current)?.id ?? list[0].id;
}

export function initialCategory(arg: string | null): CategoryId {
  return arg === 'notifications' ? 'notifications' : arg === 'skills' ? 'agents' : arg === 'usage' ? 'usage' : 'general';
}

export type PhonePage = 'list' | 'results' | 'section';

export function phonePage(query: string, category: CategoryId | null): PhonePage {
  if (normalize(query)) return 'results';
  return category ? 'section' : 'list';
}

export type EscapeAction = 'clear' | 'back' | 'close';

export function escapeAction(query: string, phone: boolean, category: CategoryId | null): EscapeAction {
  if (query) return 'clear';
  return phone && category ? 'back' : 'close';
}

export function categoryCounts(result: SearchResult): Record<CategoryId, number> {
  const out = Object.fromEntries(CATEGORIES.map((c) => [c.id, 0])) as Record<CategoryId, number>;
  for (const s of SECTIONS) {
    const h = result.hits[s.id];
    if (!h) continue;
    out[s.category] += h === 'all' ? ENTRIES.filter((e) => e.section === s.id).length : h.length;
  }
  return out;
}
