import { Drafts } from '../drafts';
import { KEY_DRAFTS_FEEDBACK } from '../storage';
import type { Kind } from './feedback';

export interface FormDraft {
  kind: Kind;
  title: string;
  description: string;
}

export const DRAFT_ID = 'new';

export const feedbackDrafts = new Drafts<FormDraft>(KEY_DRAFTS_FEEDBACK, (d) => !d.title.trim() && !d.description.trim(), { max: 1 });

interface Seed {
  kind?: Kind;
  title?: string;
  description?: string;
}

export const ownsText = (prefill: Seed | null | undefined): boolean => !prefill?.title && !prefill?.description;

export function initialForm(prefill: Seed | null | undefined, saved: FormDraft | undefined): FormDraft {
  const own = ownsText(prefill);
  return {
    kind: prefill?.kind ?? (own ? saved?.kind : undefined) ?? 'bug',
    title: prefill?.title ?? (own ? saved?.title : undefined) ?? '',
    description: prefill?.description ?? (own ? saved?.description : undefined) ?? '',
  };
}
