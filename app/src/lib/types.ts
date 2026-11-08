export type Agent = string;

export interface AgentInfo {
  id: string;
  name: string;
  installed: boolean;
  version?: string;
  models?: string[];
  efforts: Effort[];
  resume: boolean;
  verified: boolean;
  install?: string;
  sessions: boolean;
  turns: boolean;
}
export type LineState = 'active' | 'idle' | 'held' | 'hung-up';

export interface Line {
  id: string;
  owner?: string;
  ticketKey?: string | null;
  stationId?: string;
  agent: Agent;
  agentSessionId?: string;
  repo?: string;
  branch?: string;
  worktreePath?: string;
  prUrl?: string | null;
  envRef?: string | null;
  origin?: string;
  state: LineState;
  lastActivityAt: string;
}

export interface LineLocal {
  pid?: number | null;
  rssBytes?: number;
  busy?: boolean;
  needsInput?: boolean;
  pinned?: boolean;
  heldReason?: null | 'user' | 'lost';
  // When the session was held, and whether a pause waits for the current turn to end.
  heldAt?: string | null;
  holdPending?: boolean | null;
  // Running subagents and background tasks reported by hooks; 0 when unknown.
  subagents?: number;
  tasks?: number;
  orphan?: boolean;
  duplicate?: boolean;
  tmuxPane?: string;
  tmuxSession?: string;
  resumeCommand?: string;
  resumeVerified?: boolean;
  title?: string | null;
  titleOverride?: string | null;
  termId?: string | null;
  workspaceId?: string | null;
  cwd?: string;
}

export interface LocalLine {
  line: Line;
  local?: LineLocal;
}

export interface Term {
  id: string;
  pid: number;
  cwd: string;
  cmd: string[];
  title: string;
  titleOverride?: string | null;
  cols: number;
  rows: number;
  createdAt: string;
  exited: boolean;
  exitCode?: number | null;
  sizedBy?: string;
  lineId?: string | null;
  taskId?: string | null;
  workspaceId?: string | null;
  agent?: string;
  profile?: ProfileChip;
}

export interface ProfileChip {
  id?: string;
  name?: string;
  model?: string;
  effort?: string;
}

export interface SubOverride {
  model?: string;
  effort?: Effort;
}

export interface ProfileAgent {
  model?: string;
  effort?: Effort;
  subagentModel?: string;
  subagentEffort?: Effort;
  subagentForce?: boolean;
  smallModel?: string;
  subagents?: Record<string, SubOverride>;
}

export interface Profile {
  id: string;
  name: string;
  builtin?: boolean;
  agents: Record<string, ProfileAgent>;
}

export type ProfileVia = 'flag' | 'env' | 'file';

export interface ProfileSupport {
  model?: ProfileVia;
  effort?: ProfileVia;
  subagentModel?: ProfileVia;
  subagentEffort?: ProfileVia;
  smallModel?: ProfileVia;
  subagents?: ProfileVia;
  subagentEfforts?: boolean;
  subagentForce?: boolean;
}

export interface ProfilesStore {
  default: string;
  profiles: Profile[];
  scopes: Record<string, string>;
  support: Record<string, ProfileSupport>;
}

export interface SubagentDef {
  agent: string;
  name: string;
  scope: 'user' | 'project';
  path: string;
  model: string;
  effort: string;
}

export interface FileEdit {
  path: string;
  name: string;
  scope: 'user' | 'project';
  changes: { key: string; from: string; to: string }[];
  written?: boolean;
  backup?: string;
  error?: string;
}

export interface NewTerm {
  cwd?: string;
  cmd?: string[];
  agent?: string;
  model?: string;
  effort?: Effort;
  profile?: string;
  env?: Record<string, string>;
  cols?: number;
  rows?: number;
  title?: string;
  lineId?: string;
  taskId?: string;
  workspaceId?: string;
  worktree?: boolean | { branch?: string; base?: string; slug?: string };
}

export type TaskStatus = 'pending' | 'auto' | 'in_progress' | 'in_testing' | 'completed';
export type Effort = 'low' | 'medium' | 'high' | 'xhigh' | 'max';
export const TASK_STATUSES: TaskStatus[] = ['pending', 'auto', 'in_progress', 'in_testing', 'completed'];

export type TaskOrigin = 'user' | 'session' | 'agent' | 'organizer' | 'configurator';
export type TaskBadge = 'needs_input' | 'paused';

export interface Progress {
  done: number;
  total: number;
  note?: string;
}

export interface Task {
  id: string;
  number: number;
  title: string;
  description: string;
  project: string;
  status: TaskStatus;
  agent: Agent;
  model: string;
  effort: Effort;
  rank: number;
  station: string;
  ticketKey?: string | null;
  lineId?: string | null;
  termId?: string | null;
  branch?: string | null;
  worktreePath?: string | null;
  progress?: Progress | null;
  error?: string | null;
  origin?: TaskOrigin;
  urgent?: boolean | null;
  important?: boolean | null;
  parentId?: string | null;
  dependsOn?: string[];
  inheritBranch?: boolean | null;
  manualStatus?: TaskStatus | null;
  badges?: TaskBadge[];
  profile?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface TaskMessage {
  id: string;
  taskId: string;
  from: string | null;
  fromNumber?: number;
  fromTitle?: string;
  body: string;
  createdAt: string;
  delivered: boolean;
}

export interface NewTask {
  title: string;
  description: string;
  project: string;
  agent: Agent;
  model: string;
  effort?: Effort;
  ticketKey?: string;
  status?: TaskStatus;
  profile?: string;
  dependsOn?: string[];
  inheritBranch?: boolean;
  attachments?: { path: string; name?: string }[];
}

export interface Project {
  path: string;
  name: string;
  repo?: string;
  defaultBranch?: string;
  color?: string;
  missing?: boolean;
}

export type TaskSourceKind = 'github' | 'gitlab' | 'jira';

export interface TaskSource {
  id: string;
  kind: TaskSourceKind;
  target: string;
  implicit: boolean;
}

export interface SourceItem {
  source: string;
  kind: TaskSourceKind;
  key: string;
  title: string;
  body: string;
  url: string;
  ticketKey?: string | null;
}

export interface SourceItems {
  sources: TaskSource[];
  items: SourceItem[];
  errors: { source: string; error: string }[];
}

export type StepState = 'done' | 'running' | 'failed' | 'pending';

export interface WorkStep {
  id?: string;
  kind: string;
  label: string;
  path?: string;
  state: StepState;
  at: string;
}

export interface Todo {
  text: string;
  state: 'pending' | 'in_progress' | 'completed';
}

export interface Activity {
  lineId?: string;
  busy?: boolean;
  at?: string;
  title: string;
  goal: string;
  status: string;
  needsInput: boolean;
  workLog: WorkStep[];
  todos: Todo[];
  editedFiles: string[];
  lastAssistantText: string;
  transcript?: boolean;
}

export interface Diff {
  lineId: string;
  worktree: string;
  status: string;
  stat: string;
  diff: string;
  truncated: boolean;
}

export interface Status {
  stationId: string;
  version: string;
  apiConnected?: boolean;
  activeCount?: number;
  idleCount?: number;
  heldCount?: number;
  aliveCount?: number;
}

export interface Version {
  version: string;
  commit?: string;
  os?: string;
  arch?: string;
  protocol?: number;
}

export const PROTOCOL = 1;

export type EventType =
  | 'line.created'
  | 'line.updated'
  | 'line.state_changed'
  | 'line.hung_up'
  | 'line.hold_refused'
  | 'line.activity'
  | 'line.chat'
  | 'term.created'
  | 'term.updated'
  | 'term.exited'
  | 'task.created'
  | 'task.updated'
  | 'task.deleted'
  | 'task.message'
  | 'review.failed'
  | 'profiles.changed'
  | 'station.online'
  | 'station.offline'
  | 'command.requested'
  | 'command.completed'
  | 'git.changed'
  | 'worktree.created'
  | 'worktree.updated'
  | 'worktree.removed'
  | 'review.changed'
  | 'review.thread'
  | 'review.thread_deleted'
  | 'review.delivery'
  | 'project.added'
  | 'workspace.created'
  | 'workspace.updated'
  | 'workspace.deleted'
  | 'folder.created'
  | 'folder.updated'
  | 'folder.deleted'
  | 'group.created'
  | 'group.updated'
  | 'group.deleted'
  | 'broadcast.delivered'
  | 'usage.updated'
  | 'organize.proposal';

export interface DaemonEvent {
  id: string;
  type: EventType;
  at: string;
  data: any;
}

export type ConnPhase =
  | 'idle'
  | 'connecting'
  | 'checking'
  | 'downloading'
  | 'installing'
  | 'starting'
  | 'connected'
  | 'reconnecting'
  | 'error';

export type AfterReboot = 'agents' | 'shells' | 'nothing';

export interface AutostartStatus {
  platform: 'systemd' | 'launchd' | 'none';
  supported: boolean;
  installed: boolean;
  enabled: boolean;
  current: boolean;
  supervised: boolean;
  path: string | null;
  linger: boolean | null;
  lingerCommand: string | null;
}

export interface Lifecycle {
  afterReboot: AfterReboot;
  autostart: AutostartStatus;
  error?: string;
}

export interface LifecyclePatch {
  afterReboot?: AfterReboot;
  autostart?: boolean;
  linger?: boolean;
}

export interface UploadResult {
  path: string;
  short?: string;
  name: string;
  size: number;
}

export interface UploadProgress {
  sent: number;
  total: number;
}

export interface DiscoverResult {
  added: number;
  projects: Project[];
}

export interface DaemonEnv {
  path: string;
  source: string;
}

export type HookMechanism = 'hooks' | 'plugin' | 'polling';

export interface HookAgentStatus {
  agent: string;
  name: string;
  mechanism: HookMechanism;
  cliInstalled: boolean;
  configured: boolean;
  installed: boolean;
  current: boolean;
  path?: string;
  note?: string;
}

export interface SwitchboardStatus {
  found: boolean;
  dir: string | null;
  imported: boolean;
  running: boolean;
  sessions: number;
  tasks: number;
  workspaces: number;
  hooks: boolean;
  settings: Record<string, string>;
}

export interface SwitchboardResult {
  imported: boolean;
  sessions: number;
  tasks: number;
  workspaces: number;
  stopped: boolean;
  hooks: boolean;
}

export interface HooksStatus {
  shim: string;
  agents: HookAgentStatus[];
}

export interface HookResult {
  agent: string;
  path: string;
  changed: boolean;
  written: boolean;
  diff: string;
  backup?: string;
  error?: string;
  skipped?: string;
  note?: string;
}

export interface HooksRun {
  dryRun: boolean;
  results: HookResult[];
}

export interface HostTarget {
  kind: 'local' | 'ssh' | 'direct';
  host?: string;
  label: string;
}

export interface ConnState {
  phase: ConnPhase;
  target: HostTarget;
  version?: string;
  error?: string;
  attempt?: number;
  detail?: string;
  resolved?: string;
  prompt?: ConnPrompt;
  server?: ServerUpdate;
}

export interface ServerUpdate {
  running: string;
  installed: string;
  terms: number;
  reason: string;
}

export type LocalNetworkStatus = 'granted' | 'denied' | 'unreachable' | 'unknown' | 'notApplicable';

export interface LocalNetworkReport {
  status: LocalNetworkStatus;
  host?: string | null;
  endpoint?: string | null;
  via?: string | null;
  addresses: string[];
  private: boolean;
  multicast?: string | null;
  connect?: string | null;
  interface?: string | null;
  signing?: string | null;
  bundle?: string | null;
  error?: string | null;
}

export interface ConnPrompt {
  id: number;
  kind: 'hostkey' | 'secret' | 'confirm';
  text: string;
  fingerprint?: string;
  host?: string;
}

export interface Port {
  port: number;
  addr: string;
  pid: number;
  process: string;
  termId: string;
  lineId?: string;
  project?: string;
  since: string;
}

export interface Forward {
  remotePort: number;
  localPort: number;
  localAddr: string;
  url: string;
  source: 'auto' | 'manual';
  process?: string;
  pid?: number;
  termId?: string;
  project?: string;
  error?: string;
}

export interface PortsState {
  remote: boolean;
  detected: Port[];
  forwards: Forward[];
  added?: Forward[];
}

export interface PortSettings {
  autoForward: boolean;
  ignore: number[];
  localhostOnly: boolean;
}

export interface Endpoint {
  host: string;
  port: number;
  user: string;
}

export interface SavedHost extends Endpoint {
  id: string;
  label: string;
  jump?: Endpoint | null;
  auth: 'key' | 'password';
  rememberPassword: boolean;
}

export interface DeviceKey {
  publicKey: string;
  fingerprint: string;
  installCommand: string;
}

export interface PlatformInfo {
  os: string;
  mobile: boolean;
}

export interface SshHost {
  alias: string;
  hostName?: string;
  user?: string;
  port?: number;
}

export interface Workspace {
  id: string;
  name: string;
  /** Null for a subject, which has no directory of its own. */
  path: string | null;
  kind?: 'dir' | 'subject';
  /** Default working directory of a subject; null for a dir. */
  cwd?: string | null;
  station?: string;
  folderId: string | null;
  pinned: boolean;
  archived: boolean;
  auto: boolean;
  rank: number;
  tabs: string[];
  notifyIdle: boolean;
  missing?: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface Folder {
  id: string;
  name: string;
  parentId: string | null;
  rank: number;
  collapsed: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface Group {
  id: string;
  name: string;
  rank: number;
  members: string[];
  notifyIdle: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface BroadcastRequest {
  text: string;
  lineIds?: string[];
  termIds?: string[];
  workspaceId?: string;
  groupId?: string;
  onlyIdle?: boolean;
  queue?: boolean;
}

export type BroadcastOutcome = 'sent' | 'skipped' | 'queued' | 'refused';

export interface BroadcastResult {
  target: string;
  result: BroadcastOutcome;
  reason?: string;
}

export interface BroadcastResponse {
  id: string;
  results: BroadcastResult[];
}

export type ChatKind = 'user' | 'assistant' | 'tool' | 'todos' | 'notice';
export type ToolKind = 'edit' | 'write' | 'bash' | 'read' | 'search' | 'task' | 'web' | 'other';
export type ToolState = 'running' | 'done' | 'failed';

export type SubagentState = 'running' | 'done' | 'failed' | 'stopped';

export interface SubagentSummary {
  agentId: string | null;
  type: string | null;
  description: string;
  state: SubagentState;
  background: boolean;
  startedAt: string | null;
  endedAt: string | null;
  tokens: number;
  toolCount: number;
  lastActivity: string | null;
}

export interface ChatTool {
  name: string;
  kind: ToolKind;
  label: string;
  path?: string;
  command?: string;
  diff?: { old: string; new: string } | { patch: string };
  output?: string;
  truncated?: boolean;
  state: ToolState;
  detail?: string;
  agent?: SubagentSummary;
}

export interface ChatEntry {
  id: string;
  seq: number;
  rev: number;
  at: string | null;
  kind: ChatKind;
  text?: string;
  model?: string;
  images?: number;
  imagesAt?: number;
  tool?: ChatTool;
  todos?: { text: string; state: 'pending' | 'in_progress' | 'completed' }[];
}

export interface ChatPage {
  lineId: string;
  agentId?: string;
  agent: string;
  cursor: number;
  reset: boolean;
  more: boolean;
  model: string | null;
  entries: ChatEntry[];
}

export interface ScreenText {
  lineId: string;
  text: string;
  cols: number;
  rows: number;
  source: 'term' | 'tmux';
}

export interface CustomCommand {
  name: string;
  description: string;
  source: 'user' | 'project';
}
