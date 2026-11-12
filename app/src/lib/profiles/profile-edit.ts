import { NONE } from './profiles';
import type { AgentInfo, Profile, ProfileAgent, ProfileSupport } from '../types';

const MODEL_RE = /^[A-Za-z0-9._:/[\]@-]{0,128}$/;
const NAME_RE = /^[A-Za-z0-9._-]{1,64}$/;
const BUILTIN_IDS = ['economy', 'balanced', 'max'];

export function slugId(name: string, taken: string[]): string {
  const base =
    name
      .toLowerCase()
      .normalize('NFD')
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 40) || 'profile';
  const used = new Set([...taken, ...BUILTIN_IDS, NONE]);
  if (!used.has(base)) return base;
  let i = 2;
  while (used.has(`${base}-${i}`)) i++;
  return `${base}-${i}`;
}

export function clean(c: ProfileAgent): ProfileAgent {
  const out: ProfileAgent = {};
  for (const [k, v] of Object.entries(c) as [keyof ProfileAgent, unknown][]) {
    if (k === 'subagents') {
      const subs = Object.fromEntries(
        Object.entries((v as ProfileAgent['subagents']) ?? {})
          .map(([n, s]) => [n, Object.fromEntries(Object.entries(s).filter(([, x]) => !!x))])
          .filter(([, s]) => Object.keys(s as object).length),
      );
      if (Object.keys(subs).length) out.subagents = subs;
    } else if (v) (out as Record<string, unknown>)[k] = v;
  }
  return out;
}

export function validateProfile(p: Profile, support: Record<string, ProfileSupport>, agents: AgentInfo[], others: Profile[]): string[] {
  const errs: string[] = [];
  if (!NAME_RE.test(p.id) || BUILTIN_IDS.includes(p.id) || p.id === NONE) errs.push(`invalid id ${p.id}`);
  if (others.some((o) => o.id === p.id)) errs.push(`duplicate id ${p.id}`);
  const name = p.name.trim();
  if (!name || name.length > 40) errs.push('name must be 1-40 characters');
  for (const [agent, c] of Object.entries(p.agents)) {
    const s = support[agent];
    const info = agents.find((a) => a.id === agent);
    if (!s) {
      errs.push(`${agent}: unknown agent`);
      continue;
    }
    const label = info?.name ?? agent;
    const efforts: string[] = info?.efforts ?? ['low', 'medium', 'high', 'max'];
    const fields: [keyof ProfileSupport, string | undefined][] = [
      ['model', c.model],
      ['effort', c.effort],
      ['subagentModel', c.subagentModel],
      ['subagentEffort', c.subagentEffort],
      ['smallModel', c.smallModel],
    ];
    for (const [k, v] of fields) if (v && !s[k]) errs.push(`${label}: ${k} not supported`);
    if (c.subagentForce && !s.subagentForce) errs.push(`${label}: subagentForce not supported`);
    for (const m of [c.model, c.subagentModel, c.smallModel]) if (m && !MODEL_RE.test(m)) errs.push(`${label}: invalid model ${m}`);
    for (const e of [c.effort, c.subagentEffort]) if (e && !efforts.includes(e)) errs.push(`${label}: invalid effort ${e}`);
    const subs = Object.entries(c.subagents ?? {});
    if (subs.length && !s.subagents) errs.push(`${label}: subagent overrides not supported`);
    for (const [n, sub] of subs) {
      if (!NAME_RE.test(n)) errs.push(`${label}: invalid subagent ${n}`);
      if (sub.model && !MODEL_RE.test(sub.model)) errs.push(`${label}: invalid model ${sub.model}`);
      if (agent === 'agy' && sub.model && !['inherit', 'flash', 'pro'].includes(sub.model)) errs.push(`${label}: invalid Antigravity subagent model ${sub.model}`);
      if (sub.effort && (!s.subagentEfforts || !efforts.includes(sub.effort))) errs.push(`${label}: invalid effort for ${n}`);
    }
  }
  return errs;
}
