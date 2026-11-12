import type { AgentInfo, Effort, Profile, ProfileAgent, ProfileChip, ProfilesStore, ProfileSupport, TaskType } from './types';

export const NONE = 'none';
export const TASK_TYPES: TaskType[] = ['fix', 'feature', 'engineering', 'refactor', 'triage'];
export const PRICES_AS_OF = '2026-10-29';

export const PRICES: Record<string, [number, number]> = {
  fable: [10, 50],
  opus: [4, 20],
  sonnet: [2, 10],
  haiku: [0.1, 0.5],
  'gpt-5.5': [5, 30],
  'gpt-5.4': [2.5, 15],
  'gpt-5.4-mini': [0.75, 4.5],
  'gpt-5.3-codex': [1.75, 14],
  'gpt-5.2': [1.75, 14],
  'gpt-5-mini': [0.25, 2],
  'gemini-3.1-pro': [2, 12],
  'gemini-2.5-pro': [1.25, 10],
  'gemini-2.5-flash': [0.3, 2.5],
  'gemini-2.5-flash-lite': [0.1, 0.4],
};

const CLI_DEFAULT: Record<string, string> = {
  claude: 'opus',
  codex: 'gpt-5.5',
  gemini: 'gemini-2.5-pro',
  copilot: 'sonnet',
  opencode: 'sonnet',
  cursor: 'sonnet',
  agy: 'sonnet',
};

const EFFORT_FACTOR: Record<string, number> = { low: 0.5, medium: 0.75, high: 1, max: 1.6 };

function under(path: string, root: string): boolean {
  const r = root.replace(/\/+$/, '');
  return !!r && (path === r || path.startsWith(`${r}/`));
}

export function scoped(store: ProfilesStore, cwd: string | undefined): string | undefined {
  if (!cwd) return undefined;
  let best: [string, string] | undefined;
  for (const [root, id] of Object.entries(store.scopes)) {
    if (under(cwd, root) && (!best || root.length > best[0].length)) best = [root, id];
  }
  return best?.[1];
}

export function typed(store: ProfilesStore, taskType: TaskType | null | undefined): string | undefined {
  const id = taskType ? store.types?.[taskType] : undefined;
  return id && (id === NONE || store.profiles.some((p) => p.id === id)) ? id : undefined;
}

export function findProfile(store: ProfilesStore | null | undefined, id: string | undefined): Profile | undefined {
  return id ? store?.profiles.find((p) => p.id === id) : undefined;
}

type Pick = { explicit?: string; taskType?: TaskType | null; cwd?: string };

export function effectiveId(store: ProfilesStore, opts: Pick): string {
  const id = opts.explicit || typed(store, opts.taskType) || scoped(store, opts.cwd) || store.default;
  return id === NONE ? '' : id;
}

export function resolveProfile(store: ProfilesStore | null | undefined, opts: Pick): Profile | undefined {
  if (!store || opts.explicit === NONE) return undefined;
  const id = effectiveId(store, opts);
  const p = findProfile(store, id);
  if (p || !opts.explicit) return p;
  return resolveProfile(store, { taskType: opts.taskType, cwd: opts.cwd });
}

export function agentCfg(p: Profile | undefined, agent: string): ProfileAgent {
  return p?.agents[agent] ?? {};
}

export function chipLabel(chip: ProfileChip | undefined, effortLabel: (e: string) => string = (e) => `effort ${e}`): string {
  if (!chip) return '';
  return [chip.name, chip.model, chip.effort ? effortLabel(chip.effort) : ''].filter(Boolean).join(' · ');
}

export function chipShort(chip: ProfileChip | undefined): string {
  return chip?.name || chip?.model || '';
}

export function priceOf(model: string | undefined): [number, number] | undefined {
  if (!model) return undefined;
  const m = model.toLowerCase().replace(/^[a-z0-9-]+\//, '');
  if (PRICES[m]) return PRICES[m];
  for (const k of ['fable', 'opus', 'sonnet', 'haiku']) if (m.includes(k)) return PRICES[k];
  const keys = Object.keys(PRICES)
    .filter((k) => m.startsWith(k))
    .sort((a, b) => b.length - a.length);
  return keys.length ? PRICES[keys[0]] : undefined;
}

const blend = (p: [number, number]) => (3 * p[0] + p[1]) / 4;
const factor = (e: string | undefined) => EFFORT_FACTOR[e ?? ''] ?? 1;

export function sessionCost(agent: string, c: ProfileAgent, support: ProfileSupport | undefined): number | undefined {
  const main = priceOf(c.model) ?? priceOf(CLI_DEFAULT[agent]);
  if (!main) return undefined;
  const top = blend(main) * factor(c.effort);
  if (!support?.subagentModel && !support?.smallModel) return top;
  const sub = blend(priceOf(c.subagentModel) ?? main) * factor(c.subagentEffort || c.effort);
  const small = blend(priceOf(c.smallModel) ?? (agent === 'claude' ? PRICES.haiku : main));
  return 0.65 * top + 0.3 * sub + 0.05 * small;
}

export function costIndex(p: Profile | undefined, agent: string, support: ProfileSupport | undefined): number | undefined {
  const base = sessionCost(agent, {}, support);
  const cost = sessionCost(agent, agentCfg(p, agent), support);
  if (!base || cost === undefined) return undefined;
  return Math.round((cost / base) * 100) / 100;
}

export function formatIndex(x: number | undefined): string {
  if (x === undefined) return '';
  return `≈${x < 0.1 ? x.toFixed(2) : x.toFixed(x < 1 ? 2 : 1).replace(/\.?0+$/, '')}×`;
}

export function initialChoice(p: Profile | undefined, agent: AgentInfo | undefined, fallback: { model: string; effort?: Effort }): { model: string; effort?: Effort } {
  const c = agentCfg(p, agent?.id ?? '');
  const efforts = agent?.efforts ?? [];
  const model = c.model ?? fallback.model;
  if (!efforts.length) return { model };
  const want = c.effort ?? fallback.effort;
  return { model, effort: want && efforts.includes(want) ? want : efforts.includes('high') ? 'high' : efforts[efforts.length - 1] };
}
