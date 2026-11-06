import { DaemonError, type DaemonClient } from './client';
import type { StatuslineStatus } from './usage/types';
import type { DirListing } from './dirs';
import type { Activity, AgentInfo, BroadcastRequest, ChatPage, CustomCommand, ScreenText, BroadcastResponse, DaemonEnv, Diff, DiscoverResult, FileEdit, Folder, Group, HooksRun, HooksStatus, SwitchboardResult, SwitchboardStatus, LocalLine, NewTask, NewTerm, Profile, ProfilesStore, Project, Status, SubagentDef, Task, TaskMessage, Term, Version, Workspace, Lifecycle, LifecyclePatch } from './types';

const enc = encodeURIComponent;

export function api(c: DaemonClient) {
  return {
    version: () => c.request<Version>('GET', '/v1/version'),
    status: () => c.request<Status>('GET', '/v1/status'),
    lines: () => c.request<LocalLine[]>('GET', '/v1/lines'),
    hold: (id: string) => c.request('POST', `/v1/lines/${enc(id)}/hold?dryRun=false`),
    close: (id: string, shell = false) => c.request('POST', `/v1/lines/${enc(id)}/close?dryRun=false${shell ? '&shell=true' : ''}`),
    resume: (id: string) => c.request<Term>('POST', `/v1/lines/${enc(id)}/resume?dryRun=false`, { target: 'term' }),
    attach: (id: string) => c.request<Term>('POST', `/v1/lines/${enc(id)}/resume?dryRun=false`, { target: 'term', attachOnly: true }),
    pin: (id: string, pinned: boolean) => c.request('PUT', `/v1/lines/${enc(id)}/pin`, { pinned }),
    diff: (id: string) => c.request<Diff>('GET', `/v1/lines/${enc(id)}/diff`),
    activity: (id: string) => c.request<Activity>('GET', `/v1/lines/${enc(id)}/activity`),
    chat: (id: string, q: { after?: number; before?: number; limit?: number } = {}, agentId?: string) => {
      const qs = Object.entries(q)
        .filter(([, v]) => v !== undefined)
        .map(([k, v]) => `${k}=${v}`)
        .join('&');
      const sub = agentId ? `/agents/${enc(agentId)}` : '';
      return c.request<ChatPage>('GET', `/v1/lines/${enc(id)}/chat${sub}${qs ? `?${qs}` : ''}`);
    },
    screen: (id: string) => c.request<ScreenText>('GET', `/v1/lines/${enc(id)}/screen`),
    keys: (id: string, keys: string[], text?: string) => c.request<{ sent: number }>('POST', `/v1/lines/${enc(id)}/keys`, text ? { keys, text } : { keys }),
    commands: (id: string) => c.request<{ commands: CustomCommand[] }>('GET', `/v1/lines/${enc(id)}/commands`),
    terms: () => c.request<Term[]>('GET', '/v1/terms'),
    createTerm: (t: NewTerm) => c.request<Term>('POST', '/v1/terms', t),
    resizeTerm: (id: string, cols: number, rows: number) => c.request('POST', `/v1/terms/${enc(id)}/resize`, { cols, rows }),
    killTerm: (id: string) => c.request('POST', `/v1/terms/${enc(id)}/kill`, { confirm: true }),
    deleteTerm: (id: string) => c.request('DELETE', `/v1/terms/${enc(id)}`),
    tasks: () => c.request<Task[]>('GET', '/v1/tasks'),
    createTask: (t: NewTask) => c.request<Task>('POST', '/v1/tasks', t),
    patchTask: (id: string, patch: Partial<Task>) => c.request<Task>('PATCH', `/v1/tasks/${enc(id)}`, patch),
    deleteTask: (id: string) => c.request('DELETE', `/v1/tasks/${enc(id)}`),
    startTask: (id: string, opts: { inPlace?: boolean } = {}) => c.request<Task>('POST', `/v1/tasks/${enc(id)}/start`, opts.inPlace ? { inPlace: true } : undefined),
    reviewTask: async (id: string, action: 'approve' | 'reject' | 'commit', message?: string, pr: { merge?: boolean; approvePr?: boolean } = {}): Promise<Task> => {
      const r = await c.request<{ task: Task; commit?: string } | Task>('POST', `/v1/tasks/${enc(id)}/review`, { action, message, ...pr });
      return 'task' in r ? r.task : r;
    },
    taskMessages: (id: string) => c.request<TaskMessage[]>('GET', `/v1/tasks/${enc(id)}/messages`),
    sendTaskMessage: (id: string, body: string) => c.request<TaskMessage>('POST', `/v1/tasks/${enc(id)}/messages`, { body }),
    autoOrganize: () => c.request<{ enabled: boolean }>('GET', '/v1/tasks/auto-organize'),
    setAutoOrganize: (enabled: boolean) => c.request<{ enabled: boolean }>('PUT', '/v1/tasks/auto-organize', { enabled }),
    organize: (project?: string) => c.request<{ started: Task[] }>('POST', '/v1/tasks/organize', project ? { project } : {}),
    projects: () => c.request<Project[]>('GET', '/v1/projects'),
    createProject: (path: string) => c.request<Project>('POST', '/v1/projects', { path }),
    discoverProjects: () => c.request<DiscoverResult>('POST', '/v1/projects/discover'),
    agents: () => c.request<AgentInfo[]>('GET', '/v1/agents'),
    rescanAgents: () => c.request<AgentInfo[]>('POST', '/v1/agents/rescan'),
    env: () => c.request<DaemonEnv>('GET', '/v1/env'),
    hooksStatus: () => c.request<HooksStatus>('GET', '/v1/hooks/status'),
    switchboardStatus: () => c.request<SwitchboardStatus>('GET', '/v1/import/switchboard'),
    importSwitchboard: () => c.request<SwitchboardResult>('POST', '/v1/import/switchboard', {}),
    lifecycle: () => c.request<Lifecycle>('GET', '/v1/lifecycle'),
    patchLifecycle: (patch: LifecyclePatch) => c.request<Lifecycle>('PATCH', '/v1/lifecycle', patch),
    installHooks: (agents: string[], dryRun = false) => c.request<HooksRun>('POST', '/v1/hooks/install', { agents, dryRun }),
    uninstallHooks: (agents: string[], dryRun = false) => c.request<HooksRun>('POST', '/v1/hooks/uninstall', { agents, dryRun }),
    statusline: () => c.request<StatuslineStatus>('GET', '/v1/usage/statusline'),
    setStatusline: (on: boolean) => c.request<StatuslineStatus>('POST', `/v1/usage/statusline/${on ? 'install' : 'uninstall'}`, { dryRun: false }),
    profiles: () => c.request<ProfilesStore>('GET', '/v1/profiles'),
    saveProfiles: (patch: { default?: string; profiles?: Profile[]; scopes?: Record<string, string> }) => c.request<ProfilesStore>('PUT', '/v1/profiles', patch),
    subagents: (cwd?: string) => c.request<{ subagents: SubagentDef[] }>('GET', `/v1/profiles/subagents${cwd ? `?cwd=${enc(cwd)}` : ''}`),
    applyProfileFiles: (b: { profile: string; cwd?: string; dryRun: boolean; files?: string[] }) => c.request<{ dryRun: boolean; files: FileEdit[] }>('POST', '/v1/profiles/apply-files', b),
    holdPlan: (id: string) =>
      c.request<HoldPlan>('POST', `/v1/lines/${enc(id)}/hold`).catch((e) => {
        if (e instanceof DaemonError && e.status === 409 && e.body && typeof e.body === 'object') return e.body as HoldPlan;
        throw e;
      }),
    closePlan: (id: string) =>
      c.request<HoldPlan>('POST', `/v1/lines/${enc(id)}/close`).catch((e) => {
        if (e instanceof DaemonError && e.status === 409 && e.body && typeof e.body === 'object') return e.body as HoldPlan;
        throw e;
      }),
    workspaces: () => c.request<Workspace[]>('GET', '/v1/workspaces'),
    createWorkspace: (w: { path: string; name?: string; folderId?: string | null; pinned?: boolean }) => c.request<Workspace>('POST', '/v1/workspaces', w),
    patchWorkspace: (id: string, patch: Partial<Pick<Workspace, 'name' | 'folderId' | 'pinned' | 'archived' | 'rank' | 'tabs' | 'notifyIdle'>>) =>
      c.request<Workspace>('PATCH', `/v1/workspaces/${enc(id)}`, patch),
    deleteWorkspace: (id: string) => c.request('DELETE', `/v1/workspaces/${enc(id)}`),
    folders: () => c.request<Folder[]>('GET', '/v1/folders'),
    createFolder: (f: { name: string; parentId?: string | null; rank?: number }) => c.request<Folder>('POST', '/v1/folders', f),
    patchFolder: (id: string, patch: Partial<Pick<Folder, 'name' | 'parentId' | 'rank' | 'collapsed'>>) => c.request<Folder>('PATCH', `/v1/folders/${enc(id)}`, patch),
    deleteFolder: (id: string) => c.request('DELETE', `/v1/folders/${enc(id)}`),
    groups: () => c.request<Group[]>('GET', '/v1/groups'),
    createGroup: (g: { name: string; members?: string[] }) => c.request<Group>('POST', '/v1/groups', g),
    patchGroup: (id: string, patch: Partial<Pick<Group, 'name' | 'rank' | 'members' | 'notifyIdle'>> & { add?: string[]; remove?: string[] }) =>
      c.request<Group>('PATCH', `/v1/groups/${enc(id)}`, patch),
    deleteGroup: (id: string) => c.request('DELETE', `/v1/groups/${enc(id)}`),
    broadcast: (b: BroadcastRequest) => c.request<BroadcastResponse>('POST', '/v1/broadcast', b),
    dirs: (path = '', hidden = false) => c.request<DirListing>('GET', `/v1/fs/dirs?${new URLSearchParams(hidden ? { path, hidden: '1' } : { path })}`),
    fileBytes: (q: Record<string, string>) => c.fetchBytes(`/v1/file?${new URLSearchParams(q)}`),
  };
}

export type Api = ReturnType<typeof api>;

export interface HoldPlan {
  lineId?: string;
  action?: string;
  steps?: string[];
  predicates?: Record<string, { ok: boolean; detail?: string }>;
  error?: string;
}
