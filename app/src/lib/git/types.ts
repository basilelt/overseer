export type RefKind = 'branch' | 'remote' | 'tag';
export type RefsFilter = 'all' | 'current' | 'agents';

export interface GraphCommit {
  sha: string;
  parents: string[];
  author: string;
  email: string;
  at: string;
  subject: string;
}

export interface GraphRef {
  name: string;
  kind: RefKind;
  sha: string;
  upstream?: string;
  ahead?: number;
  behind?: number;
  worktree?: string;
}

export interface PrInfo {
  number: number;
  state: string;
  isDraft?: boolean;
  url: string;
}

export interface Overlay {
  branch: string;
  sha: string;
  worktree?: string;
  lineId?: string;
  agent?: string;
  state?: string;
  busy?: boolean;
  needsInput?: boolean;
  taskId?: string;
  taskNumber?: number;
  taskStatus?: string;
  ticketKey?: string;
  pr?: PrInfo;
}

export interface GraphPage {
  project: string;
  repo?: string;
  base: string;
  head: { sha: string; branch: string | null };
  refs: GraphRef[];
  overlays: Overlay[];
  commits: GraphCommit[];
  next?: string;
}

export interface CommitFile {
  path: string;
  oldPath?: string;
  status: string;
  added: number;
  removed: number;
  binary: boolean;
}

export interface CommitDetail {
  sha: string;
  parents: string[];
  author: string;
  email: string;
  at: string;
  committer: string;
  committedAt: string;
  subject: string;
  body: string;
  refs: string[];
  files: CommitFile[];
  diff: string;
  truncated: boolean;
}

export type WorktreeKind = 'main' | 'managed' | 'orphan';
export type WorktreeState = 'in_use' | 'idle' | 'stale' | 'missing';

export interface WorktreeLine {
  id: string;
  agent: string;
  state: string;
  busy: boolean;
  needsInput: boolean;
}

export interface Worktree {
  id: string;
  path: string;
  project: string;
  branch: string | null;
  head: string;
  kind: WorktreeKind;
  base: string;
  state: WorktreeState;
  dirty: boolean;
  changes: number;
  ahead?: number;
  behind?: number;
  merged: boolean;
  pushed: boolean;
  unpushed: number;
  lines: WorktreeLine[];
  termIds: string[];
  task?: { id: string; number: number; title: string; status: string };
  ticketKey?: string;
  pr?: PrInfo | null;
  sizeBytes?: number | null;
  lastActivityAt?: string;
  locked: boolean;
}

export interface WorktreeList {
  project: string;
  store: string;
  base: string;
  worktrees: Worktree[];
  orphans: Worktree[];
}

export interface NewWorktree {
  project: string;
  branch?: string;
  base?: string;
  slug?: string;
  agent?: string;
}

export interface PlanItem {
  kind: 'worktree' | 'branch' | 'prune';
  path?: string;
  name?: string;
  reason?: string;
}

export interface CleanupPlan {
  dryRun: boolean;
  id: string;
  refused: string | null;
  remove: PlanItem[];
  keep: PlanItem[];
}
