import { canInstall } from '../../agents';
import type { AgentInfo } from '../../types';

export function nextPickable(agents: AgentInfo[], from: number, dir: number): number {
  for (let i = 1; i <= agents.length; i++) {
    const j = (from + dir * i + agents.length * i) % agents.length;
    if (agents[j].installed || canInstall(agents[j])) return j;
  }
  return from;
}
