import { Drafts } from './drafts';
import { KEY_DRAFTS_TASK } from './storage';

export interface TaskDraft {
  title: string;
  description: string;
  ticketKey: string;
  attachments?: { name: string; path: string }[];
}

export const taskDrafts = new Drafts<TaskDraft>(KEY_DRAFTS_TASK, (d) => !d.title.trim() && !d.description.trim() && !d.ticketKey.trim() && !d.attachments?.length, { max: 10 });
