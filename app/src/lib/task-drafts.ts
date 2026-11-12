import { Drafts } from './drafts';
import { KEYS } from './storage';

export interface TaskDraft {
  title: string;
  description: string;
  ticketKey: string;
  attachments?: { name: string; path: string }[];
}

export const taskDrafts = new Drafts<TaskDraft>(KEYS.draftsTask, (d) => !d.title.trim() && !d.description.trim() && !d.ticketKey.trim() && !d.attachments?.length, { max: 10 });
