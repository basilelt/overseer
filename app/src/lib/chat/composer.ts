import { rank } from '../fuzzy';
import { composeRequest } from '../mobile/compose';
import type { BroadcastRequest, BroadcastResult, CustomCommand } from '../types';

export type SendMode = 'idle' | 'queue' | 'now';
export type SendResult = { kind: 'sent' } | { kind: 'queued' } | { kind: 'busy'; reason: string } | { kind: 'failed'; reason: string };

export function lineCount(text: string): number {
  let n = 1;
  for (let i = text.indexOf('\n'); i >= 0; i = text.indexOf('\n', i + 1)) n++;
  return n;
}

export function sendRequest(text: string, lineId: string, mode: SendMode): BroadcastRequest | null {
  if (mode !== 'idle') return composeRequest(text, { lineId }, mode);
  const r = composeRequest(text, { lineId }, 'queue');
  return r && { ...r, queue: false };
}

export function sendResult(results: BroadcastResult[] | undefined): SendResult {
  const r = results?.[0];
  if (!r) return { kind: 'failed', reason: 'no target' };
  if (r.result === 'sent') return { kind: 'sent' };
  if (r.result === 'queued') return { kind: 'queued' };
  if (r.result === 'skipped') return { kind: 'busy', reason: r.reason ?? 'busy' };
  return { kind: 'failed', reason: r.reason ?? r.result };
}

export function startMode(local: { busy?: boolean; needsInput?: boolean } | undefined): 'idle' | 'ask' {
  return local?.busy || local?.needsInput ? 'ask' : 'idle';
}

export interface KeyLike {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey?: boolean;
  isComposing?: boolean;
}

export function enterAction(e: KeyLike, phone: boolean): 'send' | null {
  if (e.key !== 'Enter' || e.isComposing) return null;
  if (e.ctrlKey || e.metaKey) return 'send';
  if (phone || e.shiftKey || e.altKey) return null;
  return 'send';
}

export interface SlashCommand {
  name: string;
  description: string;
  source: 'builtin' | 'user' | 'project';
}

const b = (list: [string, string][]): SlashCommand[] => list.map(([name, description]) => ({ name, description, source: 'builtin' }));

export const CLAUDE_BUILTINS = b([
  ['/add-dir', 'Add a working directory'],
  ['/advisor', 'Choose the advisor model'],
  ['/agents', 'Manage subagents'],
  ['/clear', 'Clear the conversation'],
  ['/compact', 'Compact the conversation'],
  ['/config', 'Open settings'],
  ['/context', 'Show context usage'],
  ['/cost', 'Show token usage'],
  ['/doctor', 'Check the installation'],
  ['/export', 'Export the conversation'],
  ['/help', 'Show help'],
  ['/hooks', 'Manage hooks'],
  ['/init', 'Create a CLAUDE.md'],
  ['/mcp', 'Manage MCP servers'],
  ['/memory', 'Edit memory files'],
  ['/model', 'Choose the model'],
  ['/permissions', 'Manage permissions'],
  ['/plan', 'Enter plan mode'],
  ['/review', 'Review a pull request'],
  ['/rewind', 'Rewind the conversation'],
  ['/status', 'Show status'],
  ['/todos', 'List todos'],
  ['/usage', 'Show plan usage'],
]);

export const CODEX_BUILTINS = b([
  ['/approvals', 'Choose what Codex may do'],
  ['/compact', 'Summarize the conversation'],
  ['/diff', 'Show the git diff'],
  ['/init', 'Create an AGENTS.md'],
  ['/mcp', 'List MCP tools'],
  ['/mention', 'Mention a file'],
  ['/model', 'Choose the model'],
  ['/new', 'Start a new chat'],
  ['/review', 'Review the current changes'],
  ['/status', 'Show the session configuration'],
  ['/undo', 'Undo the last turn'],
]);

export const GEMINI_BUILTINS = b([
  ['/chat', 'Manage conversation checkpoints'],
  ['/clear', 'Clear the screen'],
  ['/compress', 'Summarize the context'],
  ['/help', 'Show help'],
  ['/mcp', 'List MCP servers'],
  ['/memory', 'Manage memory'],
  ['/stats', 'Show session stats'],
  ['/tools', 'List tools'],
]);

export function commandsFor(agent: string, custom: CustomCommand[]): SlashCommand[] {
  const base = agent === 'codex' ? CODEX_BUILTINS : agent === 'gemini' ? GEMINI_BUILTINS : CLAUDE_BUILTINS;
  const names = new Set(custom.map((c) => c.name));
  return [...custom, ...base.filter((c) => !names.has(c.name))];
}

export function slashQuery(text: string, caret: number): string | null {
  const head = text.slice(0, caret);
  const m = head.match(/^\/([\w:.-]*)$/);
  return m ? m[1] : null;
}

export function slashMatches(query: string, cmds: SlashCommand[], limit = 8): SlashCommand[] {
  return rank(query, cmds, (c) => c.name.slice(1))
    .slice(0, limit)
    .map((r) => r.item);
}

export function applySlash(text: string, cmd: SlashCommand): string {
  const rest = text.replace(/^\/\S*\s?/, '');
  return `${cmd.name} ${rest}`;
}
