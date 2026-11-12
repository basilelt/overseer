import type { AgentInfo, Effort } from './types';

const ALL_EFFORTS: Effort[] = ['low', 'medium', 'high', 'max'];

export const AUTO_APPROVE_ARGS: Record<string, string[]> = {
  claude: ['--permission-mode', 'auto'],
  codex: ['--ask-for-approval', 'never'],
  gemini: ['--approval-mode=yolo'],
  agy: ['--mode=auto'],
  opencode: ['--auto'],
  copilot: ['--allow-all'],
  cursor: ['--force'],
  vibe: ['--auto-approve'],
};

const NAMES: Record<string, string> = {
  claude: 'Claude Code',
  codex: 'Codex',
  gemini: 'Gemini CLI',
  agy: 'Antigravity',
  opencode: 'opencode',
  amp: 'Amp',
  copilot: 'Copilot CLI',
  cursor: 'Cursor Agent',
  vibe: 'Mistral Vibe',
  shell: 'Shell',
};

export const SHORT_NAMES: Record<string, string> = { claude: 'Claude', codex: 'Codex', gemini: 'Gemini', vibe: 'Vibe', shell: 'Shell' };

export const FALLBACK_AGENTS: AgentInfo[] = [
  { id: 'claude', name: NAMES.claude, installed: true, models: ['opus', 'sonnet', 'haiku'], efforts: ALL_EFFORTS, resume: true, verified: true, sessions: true, turns: true },
  { id: 'codex', name: NAMES.codex, installed: true, efforts: ALL_EFFORTS, resume: true, verified: false, sessions: true, turns: true },
  { id: 'shell', name: NAMES.shell, installed: true, models: [], efforts: [], resume: false, verified: true, sessions: false, turns: false },
];

export function agentName(agents: AgentInfo[], id: string): string {
  return agents.find((a) => a.id === id)?.name ?? NAMES[id] ?? id;
}

export function findAgent(agents: AgentInfo[], id: string): AgentInfo | undefined {
  return agents.find((a) => a.id === id);
}

export function ordered(agents: AgentInfo[]): AgentInfo[] {
  return [...agents.filter((a) => a.installed), ...agents.filter((a) => !a.installed)];
}

export function splitInstalled(agents: AgentInfo[]): { installed: AgentInfo[]; missing: AgentInfo[] } {
  return { installed: agents.filter((a) => a.installed), missing: agents.filter((a) => !a.installed) };
}

export const SHELL_ID = 'shell';
export const isShellId = (id: string | null | undefined): boolean => id === SHELL_ID;
export const isAgentId = (id: string): boolean => id !== SHELL_ID;
export const isShellItem = (i: { kind: string }): boolean => i.kind === SHELL_ID;
export const isAgentItem = (i: { kind: string }): boolean => i.kind !== SHELL_ID;

export function taskAgents(agents: AgentInfo[]): AgentInfo[] {
  return ordered(agents.filter((a) => isAgentId(a.id)));
}

export function hasModel(a: AgentInfo | undefined): boolean {
  return !a || !Array.isArray(a.models) || a.models.length > 0;
}

export function efforts(a: AgentInfo | undefined): Effort[] {
  return a?.efforts ?? [];
}

export function pickAgent(agents: AgentInfo[], preferred: string, forTask = true): string {
  const list = forTask ? taskAgents(agents) : ordered(agents);
  const pref = list.find((a) => a.id === preferred && a.installed);
  return (pref ?? list.find((a) => a.installed) ?? list[0])?.id ?? preferred;
}

export interface Choice {
  model: string;
  effort?: Effort;
}

export function adaptChoice(a: AgentInfo | undefined, prev: Choice, fallbackEffort: Effort = 'high'): Choice {
  const model = a?.models?.includes(prev.model) ? prev.model : '';
  const list = efforts(a);
  if (!list.length) return { model };
  const effort = prev.effort && list.includes(prev.effort) ? prev.effort : list.includes(fallbackEffort) ? fallbackEffort : list[list.length - 1];
  return { model, effort };
}

export function sessionsAvailable(agents: AgentInfo[], id: string): boolean {
  const a = findAgent(agents, id);
  return a ? a.sessions : true;
}

export const canInstall = (a: AgentInfo): boolean => !a.installed && !!a.installable && !!a.install;
