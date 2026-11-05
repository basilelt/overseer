import type { Term } from '../types';

export type ReviewDecision = 'APPROVED' | 'CHANGES_REQUESTED' | 'REVIEW_REQUIRED' | null;

export interface ReviewRequest {
  repo: string;
  number: number;
  title: string;
  url: string;
  author: string | null;
  headRefName: string;
  baseRefName: string;
  updatedAt: string;
  isDraft: boolean;
  additions: number;
  deletions: number;
  changedFiles: number;
  reviewDecision: ReviewDecision;
  reviewed: boolean;
  myState: string | null;
  localPath: string | null;
}

export interface ReviewRequests {
  login: string;
  prs: ReviewRequest[];
  fetchedAt: number;
}

export interface StartedReview {
  term: Term;
  worktree: string;
  project: string;
  branch: string;
}

export type GhErrorCode = 'missing' | 'auth' | 'failed' | 'timeout' | 'bad';
