import type { Term } from '../types';

const MARK = 'ISSUE_URL=';
const URL_CHARS = 'A-Za-z0-9_.\\/:#-';
const RUN = new RegExp(`[${URL_CHARS}]+(?:\\n[ \\t]*[${URL_CHARS}]+)*`, 'y');
const ISSUE = /^https:\/\/github\.com\/[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+\/issues\/\d+(?:#issuecomment-\d+)?/;
const ANSI = /\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-?]*[ -/]*[@-~]|\x1b[@-Z\\-_]|[\x00-\x08\x0b-\x1f\x7f]/g;

export function stripAnsi(s: string): string {
  return s.replace(ANSI, '');
}

export function findIssueUrl(text: string): string | null {
  for (let at = text.indexOf(MARK); at >= 0; at = text.indexOf(MARK, at + 1)) {
    RUN.lastIndex = at + MARK.length;
    const run = RUN.exec(text)?.[0] ?? '';
    const closed = text.length > at + MARK.length + run.length;
    const parts = run.split('\n').map((p) => p.trim());
    const joined = parts.join('');
    const m = ISSUE.exec(joined);
    if (!m) continue;
    if (m[0].length < joined.length || closed) return m[0];
    if (parts.length > 1) {
      const head = ISSUE.exec(parts.slice(0, -1).join(''));
      if (head) return head[0];
    }
  }
  return null;
}

export class UrlWatcher {
  private tail = '';
  private decoder = new TextDecoder();

  push(bytes: Uint8Array): string | null {
    this.tail = (this.tail + this.decoder.decode(bytes, { stream: true })).slice(-8192);
    return findIssueUrl(stripAnsi(this.tail));
  }
}

export const FEEDBACK_TITLE = 'Feedback';

export function isFeedbackTerm(t: Pick<Term, 'title' | 'cmd' | 'exited' | 'taskId'>): boolean {
  return t.title === FEEDBACK_TITLE && !t.exited && !t.taskId && t.cmd.length > 1;
}

export function isComment(url: string): boolean {
  return url.includes('#issuecomment-');
}
