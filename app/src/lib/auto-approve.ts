type Api = {
  agentSettings: () => Promise<{ autoApprove: Record<string, boolean> }>;
  setAgentAutoApprove: (agent: string, autoApprove: boolean) => Promise<{ autoApprove: Record<string, boolean> }>;
};

export type AllState = 'on' | 'off' | 'mixed';

export function allState(map: Record<string, boolean>, ids: string[]): AllState {
  const values = ids.filter((id) => Object.hasOwn(map, id)).map((id) => map[id]);
  if (values.every(Boolean)) return 'on';
  if (!values.some(Boolean)) return 'off';
  return 'mixed';
}

export async function setAll(api: Api, map: Record<string, boolean>, ids: string[], on: boolean): Promise<Record<string, boolean>> {
  let next = map;
  try {
    for (const id of ids) {
      if (Object.hasOwn(next, id) && next[id] !== on) next = (await api.setAgentAutoApprove(id, on)).autoApprove;
    }
    return next;
  } catch (e) {
    const fresh = await api.agentSettings().then((r) => r.autoApprove, () => next);
    throw Object.assign(e instanceof Error ? e : new Error(String(e)), { autoApprove: fresh });
  }
}
