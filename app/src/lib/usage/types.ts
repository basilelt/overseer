export type UsageRange = 'today' | '7d' | '30d';
export type UsageBy = 'agent' | 'project' | 'model' | 'session';

export interface UsageTotals {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  total: number;
  cost: number | null;
  costPartial: boolean;
  messages: number;
  activeMs: number;
}

export interface UsageWindow {
  key: string;
  windowMinutes: number;
  usedPercent: number;
  resetsAt: string | null;
  stale: boolean;
}

export interface UsageLimits {
  source: string | null;
  observedAt?: string;
  plan?: string | null;
  windows: UsageWindow[];
  hit: { at: string; message: string } | null;
  auth?: 'ok' | 'expired' | 'unavailable';
  via?: string;
}

export interface StatuslineStatus {
  path: string;
  installed: boolean;
  command: string | null;
  changed?: boolean;
  backup?: string | null;
}

export interface UsageAgent {
  agent: string;
  today: UsageTotals;
  limits: UsageLimits;
}

export interface UsageGroup {
  key: string;
  usage: UsageTotals;
  lastAt?: string;
  agent?: string;
  sessionId?: string;
  lineId?: string | null;
  title?: string | null;
  project?: string;
  model?: string;
}

export interface UsageResponse {
  range: UsageRange;
  by: UsageBy;
  tzOffset: number;
  from: string;
  to: string;
  totals: UsageTotals;
  days: { day: string; usage: UsageTotals }[];
  groups: UsageGroup[];
  agents: UsageAgent[];
  indexing?: boolean;
}
