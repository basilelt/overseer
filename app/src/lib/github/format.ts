import type { GhErrorCode, ReviewRequest } from './types';

export interface RepoGroup {
  repo: string;
  prs: ReviewRequest[];
}

const time = (p: ReviewRequest) => Date.parse(p.updatedAt) || 0;

export function groupByRepo(prs: ReviewRequest[]): RepoGroup[] {
  const by = new Map<string, ReviewRequest[]>();
  for (const p of prs) by.set(p.repo, [...(by.get(p.repo) ?? []), p]);
  return [...by]
    .map(([repo, list]) => ({ repo, prs: list.sort((a, b) => time(b) - time(a)) }))
    .sort((a, b) => time(b.prs[0]) - time(a.prs[0]) || a.repo.localeCompare(b.repo));
}

export function sizeLabel(p: Pick<ReviewRequest, 'additions' | 'deletions'>): string {
  return `+${p.additions} −${p.deletions}`;
}

export type SizeTone = 'xs' | 's' | 'm' | 'l' | 'xl';

export function sizeTone(p: Pick<ReviewRequest, 'additions' | 'deletions'>): SizeTone {
  const n = p.additions + p.deletions;
  return n < 10 ? 'xs' : n < 100 ? 's' : n < 500 ? 'm' : n < 1000 ? 'l' : 'xl';
}

export type StateKey = 'draft' | 'approved' | 'changes' | 'reviewedByYou' | 'required';

export function reviewState(p: Pick<ReviewRequest, 'isDraft' | 'reviewDecision' | 'reviewed'>): StateKey {
  if (p.isDraft) return 'draft';
  if (p.reviewDecision === 'APPROVED') return 'approved';
  if (p.reviewDecision === 'CHANGES_REQUESTED') return 'changes';
  return p.reviewed ? 'reviewedByYou' : 'required';
}

export function ghError(e: unknown): { code: GhErrorCode; message: string; command: string | null } {
  const body = (e as { body?: { code?: string; error?: string } })?.body;
  const code = (['missing', 'auth', 'timeout', 'bad'].includes(body?.code ?? '') ? body!.code : 'failed') as GhErrorCode;
  const message = body?.error ?? String((e as Error)?.message ?? e);
  const command = code === 'auth' ? 'gh auth login' : code === 'missing' ? 'https://cli.github.com' : null;
  return { code, message, command };
}
