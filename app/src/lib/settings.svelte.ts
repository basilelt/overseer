import { installDemoStorage } from './demo';
import { isLocale, setLocale, systemLocale, type Locale } from './i18n.svelte';
import { DEFAULT_SCROLLBACK_LINES } from './scroll';
import type { Agent, Effort, HostTarget, Project } from './types';
import type { LinkTarget } from './links';
import { hostKey } from './window';
import { DEFAULT_KINDS, DEFAULT_QUIET, migrateKinds, type Delivery, type NotifyKind, type QuietHours } from './notify-prefs';

installDemoStorage();

export type ThemePref = 'auto' | 'light' | 'dark' | 'graphite' | 'solarized';
export const COLOR_TOKENS = ['accent', 'bg', 'surface', 'border', 'text', 'text-2'] as const;
export type ColorToken = (typeof COLOR_TOKENS)[number];
export type ColorPalette = 'light' | 'dark' | 'graphite' | 'solarized';
export const GRAPHITE_COLORS = { accent: '#9AB8EF', bg: '#202124', surface: '#292B30', border: '#454850', text: '#DADCE0', 'text-2': '#A8ADB7' } as const;
export const SOLARIZED_COLORS = { accent: '#70B5DB', bg: '#002b36', surface: '#073642', border: '#38606B', text: '#A7B6B6', 'text-2': '#A7B6B6' } as const;
const PRESET_COLORS = { graphite: GRAPHITE_COLORS, solarized: SOLARIZED_COLORS };
export type ColorOverrides = Partial<Record<ColorToken, string>>;
const HEX = /^#[0-9a-f]{6}$/i;

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
  workspacesLayout?: 'spaces' | 'tree';
  /** Expanded workspaces, per host. */
  spacesOpen?: Record<string, string[]>;
  /** Opened closed-intervals of the by-state view, per host. */
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
  colors: Record<'light' | 'dark', ColorOverrides> & { graphite?: ColorOverrides; solarized?: ColorOverrides };
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
  colors: { light: {}, dark: {} },
};

const KEY = 'overseer.settings';

function load(): Settings {
  try {
    const raw = globalThis.localStorage?.getItem(KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const parsed = JSON.parse(raw);
    return { ...DEFAULT_SETTINGS, ...parsed, locale: isLocale(parsed.locale) ? parsed.locale : DEFAULT_SETTINGS.locale, gpuTerminal: parsed.gpuTerminal === true, notifyKinds: migrateKinds(parsed), notifyQuiet: { ...DEFAULT_QUIET, ...parsed.notifyQuiet } };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

function urlTheme(): ThemePref | null {
  const v = globalThis.location ? new URLSearchParams(globalThis.location.search).get('theme') : null;
  return v === 'light' || v === 'dark' || v === 'auto' || v === 'graphite' || v === 'solarized' ? v : null;
}

export const settings = $state<Settings>({ ...load(), ...(urlTheme() ? { theme: urlTheme()! } : {}) });

export function saveSettings(patch: Partial<Settings>) {
  Object.assign(settings, patch);
  if (patch.locale) setLocale(patch.locale);
  try {
    globalThis.localStorage?.setItem(KEY, JSON.stringify({ ...load(), ...$state.snapshot(patch) }));
  } catch {}
  applyTheme();
}

export function syncSettings(raw: string | null) {
  if (!raw) return;
  try {
    const next = JSON.parse(raw) as Record<string, unknown>;
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

export function resolvedTheme(): 'light' | 'dark' {
  return settings.theme === 'auto' ? (systemDark ? 'dark' : 'light') : settings.theme === 'graphite' || settings.theme === 'solarized' ? 'dark' : settings.theme;
}

export function colorPalette(): ColorPalette {
  return settings.theme === 'graphite' || settings.theme === 'solarized' ? settings.theme : resolvedTheme();
}

export function colorOverrides(palette = colorPalette()): ColorOverrides {
  const raw = settings.colors?.[palette] ?? {};
  return Object.fromEntries(COLOR_TOKENS.filter((k) => HEX.test(raw[k] ?? '')).map((k) => [k, raw[k]]));
}

export function customColors(palette: ColorPalette = colorPalette()): ColorOverrides {
  return palette === 'graphite' || palette === 'solarized' ? { ...PRESET_COLORS[palette], ...colorOverrides(palette) } : colorOverrides(palette);
}

export function applyTheme() {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  root.dataset.theme = resolvedTheme();
  if (settings.theme === 'graphite' || settings.theme === 'solarized') root.dataset.palette = settings.theme;
  else delete root.dataset.palette;
  const colors = customColors();
  for (const k of COLOR_TOKENS) {
    const v = colors[k];
    if (v) root.style.setProperty(`--${k}`, v);
    else root.style.removeProperty(`--${k}`);
  }
}

setLocale(settings.locale);
