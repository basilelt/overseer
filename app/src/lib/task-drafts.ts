import { Drafts } from './drafts';

export interface TaskDraft {
  title: string;
  description: string;
  ticketKey: string;
  attachments?: { name: string; path: string }[];
}

export const taskDrafts = new Drafts<TaskDraft>('overseer.drafts.task', (d) => !d.title.trim() && !d.description.trim() && !d.ticketKey.trim() && !d.attachments?.length, { max: 10 });
