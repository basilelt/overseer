import type { Effort, Task, TaskType } from '../types';

export interface EditFields {
  title: string;
  description: string;
  agent: string;
  model: string;
  effort: Effort | undefined;
  hasModel: boolean;
  effortOptions: Effort[];
  ticketKey: string;
  taskType: TaskType | '' | null | undefined;
  /** '' means inherit the profile, 'none' means no profile. */
  profile: string;
}

/**
 * The PATCH body for the fields the task detail's edit form changed, or null
 * when nothing changed. Only the fields that differ from the task go in, so a
 * save never rewrites the rest. An empty title is the form's error, not a
 * patch: the daemon refuses it.
 */
export function editPatch(task: Task, f: EditFields): Partial<Task> | null {
  const out: Partial<Task> = {};
  const title = f.title.trim();
  if (title && title !== task.title) out.title = title;
  if (f.description !== task.description) out.description = f.description;
  if (f.agent && f.agent !== task.agent) out.agent = f.agent;
  if (f.hasModel) {
    if (f.model !== task.model) out.model = f.model;
  } else if (task.model) out.model = '';
  if (!f.effortOptions.length) {
    if (task.effort) out.effort = '' as Effort;
  } else if (f.effort && f.effort !== task.effort) out.effort = f.effort;
  const ticket = f.ticketKey.trim();
  if (ticket !== (task.ticketKey ?? '')) out.ticketKey = ticket || null;
  const ty = f.taskType || null;
  if (ty !== (task.taskType ?? null)) out.taskType = ty;
  const profile = f.profile === '' ? null : f.profile;
  if (profile !== (task.profile ?? null)) out.profile = profile;
  return Object.keys(out).length ? out : null;
}
