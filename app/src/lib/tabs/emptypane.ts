import type { AgentInfo, Workspace } from '../types';
import { isAgentId } from '../agents';

export interface EmptyPaneInput {
  sidebarMode: string;
  workspaceId: string;
  workspace: Workspace | null;
  members: number;
  agents: AgentInfo[];
  defaultAgent: string;
  connected: boolean;
}

export interface EmptyPaneView {
  workspace: { name: string; path: string; missing: boolean } | null;
  noSessions: boolean;
  agent: AgentInfo | null;
  canStart: boolean;
}

export function pickAgent(agents: AgentInfo[], preferred: string): AgentInfo | null {
  const usable = agents.filter((a) => a.installed && isAgentId(a.id));
  return usable.find((a) => a.id === preferred) ?? usable[0] ?? null;
}

export function emptyPane(o: EmptyPaneInput): EmptyPaneView {
  const scoped = o.sidebarMode === 'workspaces' && !o.workspaceId.startsWith('group:') && !!o.workspace && o.workspace.id === o.workspaceId;
  const ws = scoped ? o.workspace! : null;
  const missing = !!ws?.missing;
  return {
    workspace: ws ? { name: ws.name, path: ws.path ?? ws.cwd ?? '', missing } : null,
    noSessions: !!ws && o.members === 0,
    agent: pickAgent(o.agents, o.defaultAgent),
    canStart: o.connected && !missing,
  };
}
