import { afterEach, describe, expect, it } from 'vitest';
import { taskDrafts } from './task-drafts';

const blank = { title: '', description: '', ticketKey: '' };

afterEach(() => taskDrafts.delete('t'));

describe('task drafts', () => {
  it('keeps a draft that only has attachments', () => {
    const d = { ...blank, attachments: [{ name: 'shot.png', path: '/h/.overseer/u/ab12cd.png' }] };
    taskDrafts.set('t', d);
    expect(taskDrafts.get('t')).toEqual(d);
  });

  it('drops a draft with no text and no attachments', () => {
    taskDrafts.set('t', { ...blank, title: 'x' });
    taskDrafts.set('t', { ...blank, attachments: [] });
    expect(taskDrafts.get('t')).toBeUndefined();
  });
});
