export type FileStatus = 'added' | 'modified' | 'deleted' | 'renamed' | 'copied' | 'untracked' | 'submodule';
export type BaseSource = 'explicit' | 'task' | 'created' | 'upstream' | 'forkpoint' | 'default';
export type TagKind = 'move' | 'format' | 'imports' | 'ident' | 'rename' | 'generated' | 'test' | 'comment' | 'logic' | 'unknown';
export type LineKind = 'ctx' | 'add' | 'del';
export type Side = 'new' | 'old';
export type ThreadStatus = 'pending' | 'sent' | 'addressed' | 'resolved';
export type SubmitKind = 'comment' | 'request_changes' | 'approve';
export type DeliveryStatus = 'delivered' | 'queued' | 'none';

export interface ReviewLine {
  kind: LineKind;
  old?: number;
  new?: number;
  text: string;
}

export interface ReviewHunk {
  header: string;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: ReviewLine[];
  tags?: Tag[];
}

export interface HunkRef {
  path: string;
  hunk: number;
}

export interface Tag {
  kind: TagKind;
  from: number;
  to: number;
  ref?: HunkRef;
  sig?: boolean;
}

export interface Triage {
  changed: number;
  byKind: Partial<Record<TagKind, number>>;
  lowRiskPct: number;
}

export interface ReviewFile {
  path: string;
  oldPath: string;
  status: FileStatus;
  binary: boolean;
  additions: number;
  deletions: number;
  hash: string;
  viewed: boolean;
  truncated: boolean;
  hunks: ReviewHunk[];
  submodule?: { from: string | null; to: string | null } | null;
  risk?: number;
  residual?: number;
}

export interface ReviewCommit {
  sha: string;
  subject: string;
  author: string;
  at: string;
}

export interface ReviewDiff {
  worktree: string;
  branch?: string | null;
  base?: string | null;
  baseSource?: BaseSource | null;
  superproject?: string | null;
  linked?: { worktree: string; branch: string | null }[];
  mergeBase?: string | null;
  head?: string | null;
  revision: string;
  truncated: boolean;
  readOnly?: boolean;
  commits?: ReviewCommit[];
  recent?: ReviewCommit[];
  files: ReviewFile[];
  triage?: Triage;
}

export type GraphNodeKind = 'fn' | 'type' | 'class' | 'impl' | 'mod' | 'const' | 'component' | 'file';

export interface GraphNode {
  id: string;
  kind: GraphNodeKind;
  name: string;
  path: string;
  line: number;
  endLine: number;
  hunks: HunkRef[];
  add: number;
  del: number;
  tag: TagKind;
  risk: number;
}

export interface GraphEdge {
  from: string;
  to: string;
  kind: 'uses' | 'imports';
}

export interface ReviewGraph {
  revision: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
  order: string[];
  truncated: boolean;
}

export interface MapFile {
  path: string;
  status: string;
  size: number;
  add: number;
  del: number;
  logic: number;
  noise: number;
  sig: number;
  edits: number;
  fails: number;
  viewed: boolean;
  threads: number;
  test: boolean;
}

export interface MapRegion {
  worktree: string;
  name: string;
  branch: string | null;
  parent: number | null;
  error: string | null;
  truncated: boolean;
  changed: MapFile[];
  tree: [string, number][];
}

export interface MapEnd {
  w: number;
  path: string;
}

export interface MapLink {
  from: MapEnd;
  to: MapEnd;
  kind: 'uses' | 'imports';
  count: number;
  cross: boolean;
  probable?: boolean;
}

export interface ReviewMap {
  regions: MapRegion[];
  links: MapLink[];
}

export interface RelatedWorktree {
  path: string;
  branch?: string | null;
  head?: string | null;
  base?: string | null;
  files: number;
  dirty: number;
  ahead: number;
  agents: number;
  current: boolean;
  main: boolean;
  at: string;
}

export interface Related {
  current: string;
  worktrees: RelatedWorktree[];
}

export interface ReviewBranch {
  name: string;
  head: string;
  at: string;
  worktree?: string | null;
  subject: string;
}

export interface Branches {
  base: string | null;
  branches: ReviewBranch[];
}

export interface BaseRef {
  name: string;
  remote: boolean;
  head: string;
  at: string;
  subject: string;
}

export interface Bases {
  base: string | null;
  baseSource?: BaseSource | null;
  default: string;
  branches: BaseRef[];
}

export interface ReviewComment {
  id: string;
  author: 'user' | 'agent';
  body: string;
  pending: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface Delivery {
  id: string;
  status: DeliveryStatus;
  kind: SubmitKind;
  target?: string | null;
  reason?: string | null;
  at: string;
  threads: string[];
}

export interface Thread {
  id: string;
  worktree: string;
  path: string;
  side: Side;
  line: number;
  endLine: number;
  originalLine: number;
  originalEndLine: number;
  code: string;
  status: ThreadStatus;
  outdated: boolean;
  changed: boolean;
  lineId?: string | null;
  taskId?: string | null;
  delivery?: Delivery | null;
  createdAt: string;
  updatedAt: string;
  comments: ReviewComment[];
}

export interface SubmitResult {
  delivery: Delivery;
  threads: Thread[];
  text: string;
}

export interface ReviewTarget {
  worktree?: string;
  lineId?: string;
  taskId?: string;
  branch?: string;
}

export interface NewThread extends ReviewTarget {
  path: string;
  side?: Side;
  line: number;
  endLine?: number;
  body: string;
  base?: string;
}
