import { installDemoStorage } from '../daemon/demo';
import { isLocale, setLocale, systemLocale, type Locale } from '../i18n.svelte';
import { DEFAULT_SCROLLBACK_LINES } from '../terminal/scroll';
import type { Agent, Effort, HostTarget, Project } from '../types';
import type { LinkTarget } from '../links';
import type { HostChoice, HostWindows } from '../host/host-choice';
import { hostKey } from '../window';
import { DEFAULT_KINDS, DEFAULT_QUIET, migrateKinds, type Delivery, type NotifyKind, type QuietHours } from '../notify/notify-prefs';

installDemoStorage();

import { migrateTheme, modeOf, resolveTheme, COLOR_TOKENS, type ColorOverrides, type CustomPreset, type Mode, type ResolvedTheme, type ThemePref } from '../theme/theme';

import { readItem, writeJson, KEY_SETTINGS } from '../storage';


export interface Settings {
  theme: ThemePref;
  locale: Locale;
  fontSize: number;
  shiftEnterNewline: boolean;
  ctrlWClosesTab: boolean;
  newTabShell: boolean;
  wrapTabs: boolean;
  terminalFont: string;
  lineHeight: number;
  scrollbackLines: number;
  gpuTerminal: boolean;
  sidebarWidth: number;
  panelWidth: number;
  panelOrder: string[];
  defaultAgent: Agent;
  defaultModel: string;
  defaultEffort: Effort;
  autoConcurrency: number;
  autoOrganize: boolean;
  agentMessaging: boolean;
  agentTasks: boolean;
  kanbanView: 'board' | 'matrix';
  recentHosts: HostTarget[];
  serverBinDir: string;
  extraProjects: Project[];
  projectColors: Record<string, string>;
  notifyKinds: Record<NotifyKind, Delivery>;
  notifyBackgroundToasts: boolean;
  notifySound: boolean;
  notifyPreview: boolean;
  notifyOnlyUnfocused: boolean;
  notifySkipWatched: boolean;
  notifyCooldown: number;
  notifyQuiet: QuietHours;
  notifyMuted: Record<string, string[]>;
  hooksNever: string[];
  keepRunningOffered?: string[];
  sidebarMode?: 'workspaces' | 'status';
  sidebarGit?: boolean;
  spacesOpen?: Record<string, string[]>;
  statesOpen?: Record<string, string[]>;
  workspaceId?: string;
  workspaceByHost?: Record<string, string>;
  portsAutoForward: boolean;
  portsIgnore: string;
  portsLocalhostOnly: boolean;
  sessionsInCheckout: boolean;
  localNetworkAsked: boolean;
  lastDirect?: HostTarget;
  lastHost?: HostTarget;
  explicitlyDisconnected?: boolean;
  phoneFontSize?: number;
  hideSessionTasks?: boolean;
  feedbackRepo?: string;
  tourSeen?: number | string;
  organizeNudgeAt?: number;
  fetchClaudeLimits: boolean;
  linkTarget: LinkTarget;
  linkBrowser: string;
  linkAsk: boolean;
  lightPreset: string;
  darkPreset: string;
  customPresets: CustomPreset[];
  hostWindows: HostWindows;
  hostAsk: boolean;
  hostChoice: Record<string, HostChoice>;
  hostTabColor: boolean;
  hostReconnect: boolean;
  windowHosts?: HostTarget[];
  hostsFolded?: string[];
}

export const DEFAULT_SETTINGS: Settings = {
  theme: 'auto',
  locale: systemLocale(),
  fontSize: 13,
  shiftEnterNewline: true,
  ctrlWClosesTab: true,
  newTabShell: false,
  wrapTabs: false,
  terminalFont: '',
  lineHeight: 1,
  scrollbackLines: DEFAULT_SCROLLBACK_LINES,
  gpuTerminal: false,
  sidebarWidth: 288,
  panelWidth: 340,
  panelOrder: [],
  defaultAgent: 'claude',
  defaultModel: '',
  defaultEffort: 'high',
  autoConcurrency: 2,
  autoOrganize: false,
  agentMessaging: true,
  agentTasks: true,
  kanbanView: 'board',
  recentHosts: [],
  serverBinDir: '',
  extraProjects: [],
  projectColors: {},
  notifyKinds: DEFAULT_KINDS,
  notifyBackgroundToasts: false,
  notifySound: true,
  notifyPreview: true,
  notifyOnlyUnfocused: false,
  notifySkipWatched: true,
  notifyCooldown: 20,
  notifyQuiet: DEFAULT_QUIET,
  notifyMuted: {},
  hooksNever: [],
  sessionsInCheckout: false,
  portsAutoForward: true,
  portsIgnore: '22',
  portsLocalhostOnly: true,
  localNetworkAsked: false,
  fetchClaudeLimits: false,
  linkTarget: 'overseer',
  linkBrowser: '',
  linkAsk: false,
  lightPreset: 'light',
  darkPreset: 'dark',
  customPresets: [],
  hostWindows: 'separate',
  hostAsk: false,
  hostChoice: {},
  hostTabColor: true,
  hostReconnect: true,
};

const KEY = KEY_SETTINGS;

function load(): Settings {
  try {
    const raw = readItem(KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const parsed = JSON.parse(raw);
    delete parsed.colors;
    return { ...DEFAULT_SETTINGS, ...parsed, ...migrateTheme(JSON.parse(raw)), locale: isLocale(parsed.locale) ? parsed.locale : DEFAULT_SETTINGS.locale, gpuTerminal: parsed.gpuTerminal === true, notifyKinds: migrateKinds(parsed), notifyQuiet: { ...DEFAULT_QUIET, ...parsed.notifyQuiet } };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

function urlTheme(): Partial<Settings> | null {
  const v = globalThis.location ? new URLSearchParams(globalThis.location.search).get('theme') : null;
  if (v === 'light' || v === 'dark' || v === 'auto') return { theme: v };
  return v === 'graphite' || v === 'solarized' ? { theme: 'dark', darkPreset: v } : null;
}

export const settings = $state<Settings>({ ...load(), ...urlTheme() });

export function saveSettings(patch: Partial<Settings>) {
  Object.assign(settings, patch);
  if (patch.locale) setLocale(patch.locale);
  writeJson(KEY, { ...load(), ...$state.snapshot(patch) });
  applyTheme();
}

export function syncSettings(raw: string | null) {
  if (!raw) return;
  try {
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const next = { ...parsed, ...migrateTheme(parsed) } as Record<string, unknown>;
    delete next.colors;
    const now = settings as unknown as Record<string, unknown>;
    const changed = Object.keys(next).filter((k) => JSON.stringify(next[k]) !== JSON.stringify(now[k]));
    if (!changed.length) return;
    for (const k of changed) now[k] = next[k];
    if (changed.includes('locale')) setLocale(settings.locale);
    applyTheme();
  } catch {}
}

globalThis.addEventListener?.('storage', (e) => {
  const s = e as StorageEvent;
  if (s.key === KEY) syncSettings(s.newValue);
});

export function rememberHost(h: HostTarget, main = true) {
  const last = main ? { lastHost: h, explicitlyDisconnected: false } : {};
  if (h.kind === 'direct') return saveSettings(main ? { lastDirect: h, ...last } : {});
  if (h.kind !== 'ssh') return saveSettings(last);
  const rest = settings.recentHosts.filter((x) => x.host !== h.host);
  saveSettings({ recentHosts: [h, ...rest].slice(0, 8), ...last });
}

export function workspaceFor(h: HostTarget): string | undefined {
  return settings.workspaceByHost?.[hostKey(h)] ?? settings.workspaceId;
}

export function rememberWorkspace(h: HostTarget, id: string) {
  saveSettings({ workspaceByHost: { ...settings.workspaceByHost, [hostKey(h)]: id } });
}

let systemDark = $state(globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches ?? true);
globalThis.matchMedia?.('(prefers-color-scheme: dark)').addEventListener('change', (e) => setSystemDark(e.matches));
globalThis.addEventListener?.('overseer:system-theme', (e) => setSystemDark(!!(e as CustomEvent<{ dark?: boolean }>).detail?.dark));

export function setSystemDark(dark: boolean) {
  systemDark = dark;
  applyTheme();
}

export function currentTheme(): ResolvedTheme {
  return resolveTheme(settings, systemDark);
}

export function resolvedTheme(): Mode {
  return modeOf(settings.theme, systemDark);
}

export function colorPalette() {
  return currentTheme().palette;
}

export function customColors(): ColorOverrides {
  return currentTheme().colors;
}

export function applyTheme() {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  const r = currentTheme();
  root.dataset.theme = r.mode;
  if (r.custom) root.dataset.palette = 'custom';
  else if (r.palette === 'graphite' || r.palette === 'solarized') root.dataset.palette = r.palette;
  else delete root.dataset.palette;
  for (const k of COLOR_TOKENS) {
    const v = r.colors[k];
    if (v) root.style.setProperty(`--${k}`, v);
    else root.style.removeProperty(`--${k}`);
  }
  if (r.accentFg) root.style.setProperty('--accent-fg', r.accentFg);
  else root.style.removeProperty('--accent-fg');
}

setLocale(settings.locale);
