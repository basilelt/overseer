import type { Project } from './types';

export * from './feedback-watch';

export const DEFAULT_REPO = 'basilelt/overseer';
export const PRIVACY_URL = `https://github.com/${DEFAULT_REPO}/blob/main/PRIVACY.md`;
export const REPORT = '@REPORT@';
export const ATTACHMENTS = '@ATTACHMENTS@';
export const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'];
export const VIDEO_EXTS = ['mp4', 'mov', 'webm'];
export const ATTACH_ACCEPT = [...IMAGE_EXTS, ...VIDEO_EXTS].map((e) => `.${e}`).join(',');
export const IMAGE_MAX = 10 * 1024 * 1024;
export const VIDEO_MAX = 100 * 1024 * 1024;
export const VIDEO_FREE_MAX = 10 * 1024 * 1024;
export const APP_LINES = 200;
export const SERVER_LINES = 100;
export const MAX_BODY = 60_000;
const MAX_URL_BODY = 6_000;

export type Kind = 'bug' | 'idea' | 'other';

export const KIND_PREFIX: Record<Kind, string> = { bug: 'Bug', idea: 'Idea', other: 'Feedback' };
export const KIND_LABEL: Record<Kind, string> = { bug: 'bug', idea: 'enhancement', other: 'feedback' };

export interface Form {
  kind: Kind;
  title: string;
  description: string;
  attachments: string[];
  diagnostics: string | null;
}

export type Problem = 'description' | 'repo' | 'tooLong';

export type AttachProblem = 'type' | 'image' | 'video';

export function attachProblem(name: string, size = 0): AttachProblem | null {
  const ext = /\.([A-Za-z0-9]{1,10})$/.exec(name)?.[1]?.toLowerCase() ?? '';
  if (IMAGE_EXTS.includes(ext)) return size > IMAGE_MAX ? 'image' : null;
  if (VIDEO_EXTS.includes(ext)) return size > VIDEO_MAX ? 'video' : null;
  return 'type';
}

export function validRepo(repo: string): boolean {
  return /^(?![-.])[A-Za-z0-9_.-]+\/(?![-.])[A-Za-z0-9_.-]+$/.test(repo.trim());
}

export function validate(f: Form, repo: string): Problem | null {
  if (!f.description.trim()) return 'description';
  if (!validRepo(repo)) return 'repo';
  if (buildBody(f).length > MAX_BODY) return 'tooLong';
  return null;
}

export function issueTitle(f: Pick<Form, 'kind' | 'title' | 'description'>): string {
  const own = redact(f.title.trim() || f.description.trim().split('\n')[0].trim());
  const short = own.length > 80 ? `${own.slice(0, 79).trimEnd()}…` : own;
  return `${KIND_PREFIX[f.kind]}: ${short}`;
}

export function wantsDiagnostics(kind: Kind): boolean {
  return kind !== 'idea';
}

export function labelsFor(kind: Kind): string[] {
  return kind === 'other' ? ['feedback'] : [KIND_LABEL[kind], 'feedback'];
}

const SECRET_KEYS = 'access_token|refresh_token|token|password|passwd|passphrase|secret|api_key|apikey|authorization|cookie';

export function redact(s: string): string {
  return s
    .replace(/(\b[a-z][a-z0-9+.-]*:\/\/[^\s:/@]+:)[^\s@/]+(@)/gi, '$1***$2')
    .replace(/\b(bearer)\s+[^\s"']+/gi, '$1 ***')
    .replace(new RegExp(`(^|[^A-Za-z0-9])(${SECRET_KEYS})(["']?\\s*[=:]\\s*["']?)[^\\s"',;&})\\]]+`, 'gi'), (_m, pre: string, k: string, sep: string) => `${pre}${k}${sep}***`)
    .replace(/(^|[^A-Za-z0-9])(github_pat_|gh[pousr]_)\w{8,}/g, '$1$2***')
    .replace(/(^|[^\w])sk-[\w-]{16,}/g, '$1sk-***');
}

export interface DiagInput {
  appVersion: string;
  os: string;
  arch: string;
  serverVersion: string;
  hostOs: string;
  hostArch: string;
  mode: string;
  crash?: string | null;
  appLog: string[];
  serverLog: string[];
}

export function buildDiagnostics(d: DiagInput): string {
  const head = [
    `App: ${d.appVersion || '?'} (${[d.os, d.arch].filter(Boolean).join(' ') || '?'})`,
    `Server: ${d.serverVersion || '?'} (${[d.hostOs, d.hostArch].filter(Boolean).join(' ') || '?'})`,
    `Connection: ${d.mode || '?'}`,
  ];
  const parts = [head.join('\n')];
  if (d.crash) parts.push(`--- Crash report ---\n${d.crash.trimEnd()}`);
  parts.push(`--- App log (last ${Math.min(d.appLog.length, APP_LINES)} lines) ---\n${d.appLog.slice(-APP_LINES).join('\n') || '(empty)'}`);
  parts.push(`--- Server log (last ${Math.min(d.serverLog.length, SERVER_LINES)} lines) ---\n${d.serverLog.slice(-SERVER_LINES).join('\n') || '(empty)'}`);
  return redact(parts.join('\n\n'));
}

function fence(text: string): string {
  const longest = Math.max(2, ...Array.from(text.matchAll(/`+/g), (m) => m[0].length));
  const f = '`'.repeat(longest + 1);
  return `${f}text\n${text}\n${f}`;
}

export function buildBody(f: Form, upload = false): string {
  const out = [`**Type:** ${KIND_PREFIX[f.kind]}`, '', f.description.trim()];
  if (f.attachments.length) out.push('', '### Attachments', ...(upload ? [ATTACHMENTS] : f.attachments.map((p) => `- \`${p}\``)));
  if (f.diagnostics) out.push('', '<details><summary>Diagnostics</summary>', '', fence(f.diagnostics), '', '</details>');
  out.push('', '_Sent from Overseer._');
  return redact(out.join('\n')) + '\n';
}

export interface PromptInput {
  repo: string;
  kind: Kind;
  title: string;
  attachments: string[];
  investigate: boolean;
  inClone: boolean;
}

export function buildPrompt(p: PromptInput): string {
  const label = labelsFor(p.kind);
  const lines = [
    `File one piece of Overseer user feedback as a GitHub issue in ${p.repo}, on behalf of the user. Use their own \`gh\` login; never print or ask for tokens.`,
    '',
    `The user's report (Markdown, already redacted) is in ${REPORT}. Read it first.`,
    `Suggested title: ${p.title}`,
    '',
    '1. Run `gh auth status`. If gh is missing or not logged in, stop and tell the user to run `gh auth login`.',
    `2. Look for duplicates: \`gh issue list --repo ${p.repo} --state all --search "<keywords>" --limit 20\`. If an open issue already describes the same thing, add the report as a comment with \`gh issue comment <number> --repo ${p.repo} --body-file <file>\` instead of creating a new issue.`,
    `3. Otherwise create exactly ONE issue with \`gh issue create --repo ${p.repo} --title "<title>" --body-file <file>\`:`,
    `   - Title starts with "${KIND_PREFIX[p.kind]}: ", short and specific.`,
    "   - Body: keep the user's words (fix only typos and formatting).",
  ];
  if (p.kind === 'bug') lines.push('   - Add "Steps to reproduce", "Expected" and "Actual" sections from the report; write "Not specified" when the report does not say. Do not invent details.');
  lines.push(
    '   - Keep the <details> Diagnostics block exactly as it is in the report.',
    `   - Labels: ${label.map((l) => `\`${l}\``).join(', ')}. Check \`gh label list --repo ${p.repo}\`; create a missing label with \`gh label create\` only if you have permission. If labels fail, create the issue without them.`,
    '   - Write the final body to a new file from `mktemp`, never inside a repository.',
  );
  if (p.attachments.length)
    lines.push(
      `   - Attachments (local files): ${p.attachments.join(', ')}. Pass each one with \`--attach <path>\` on the \`gh issue create\` (or \`gh issue comment\`) command; gh uploads it to the issue. Check \`gh issue create --help\` lists \`--attach\`; if gh is too old (needs 2.99+), keep the "Attachments" list of paths from the report and tell the user to update gh or drag the files into the issue. If gh refuses a file, keep it listed under "Attachments" with the reason. Once attached, replace that list in the body with "Attached below."`,
    );
  if (p.investigate)
    lines.push(
      `4. Before filing, investigate read-only: read the diagnostics, then search ${p.inClone ? 'the code in this checkout' : 'the logs in ~/.overseer'} for the likely cause, and add a short "Likely cause" section with file paths. Read only.`,
    );
  lines.push(
    '',
    'Do not edit, create or delete files in any repository, and do not commit or push.',
    'When done, print the URL of the new issue (or of your comment) as the very last line, exactly in this form: ISSUE_URL=<url>',
  );
  return lines.join('\n');
}

export function repoSlug(remote: string | undefined): string | null {
  if (!remote) return null;
  const m = /github\.com[:/]+([^/\s]+)\/([^/\s]+?)(?:\.git)?\/?$/i.exec(remote.trim());
  return m ? `${m[1]}/${m[2]}`.toLowerCase() : null;
}

export function findClone(projects: Project[], repo: string): string | undefined {
  const want = repo.trim().toLowerCase();
  return projects.find((p) => !p.missing && repoSlug(p.repo) === want)?.path;
}

export function newIssueUrl(repo: string, title: string, body: string): string {
  const b = body.length > MAX_URL_BODY ? `${body.slice(0, MAX_URL_BODY)}\n\n(truncated: the full report was copied to the clipboard)` : body;
  return `https://github.com/${repo}/issues/new?title=${encodeURIComponent(title)}&body=${encodeURIComponent(b)}`;
}
