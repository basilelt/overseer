import { describe, expect, it } from 'vitest';
import { parseStyle } from '../../test/css';
import { columns } from './kanban';
import { TASK_STATUSES } from './task-status';
import { en } from '../i18n/en';
import column from '../components/kanban/KanbanColumn.svelte?raw';
import depPicker from '../components/kanban/DepPicker.svelte?raw';
import view from '../components/kanban/KanbanView.svelte?raw';
import type { TaskStatus } from '../types';

const NO_COLUMN: TaskStatus[] = [];
const BASE_COLOUR_ONLY: Record<string, TaskStatus[]> = { KanbanColumn: ['pending'], DepPicker: ['pending', 'auto'], KanbanView: ['pending'] };

const classes: [string, string, string[]][] = [
  ['KanbanColumn', column, ['col']],
  ['DepPicker', depPicker, ['dot']],
  ['KanbanView', view, ['chipbtn', 'sdot']],
];

describe('task status consistency across files (parsed CSS)', () => {
  it('gives every status a board column unless listed as columnless', () => {
    const shown = columns([]).map((c) => c.status);
    expect(TASK_STATUSES.filter((s) => !shown.includes(s))).toEqual(NO_COLUMN);
  });

  it('has an English column label for every status', () => {
    for (const s of TASK_STATUSES) expect(en, s).toHaveProperty([`kanban.col.${s}`]);
  });

  it.each(classes)('%s colours every status by class unless it uses the base colour', (name, source, bases) => {
    const selectors = new Set(parseStyle(source).map((r) => r.selector));
    for (const s of TASK_STATUSES) {
      if (BASE_COLOUR_ONLY[name].includes(s)) continue;
      for (const b of bases) expect(selectors.has(`.${b}.${s}`), `${b}.${s}`).toBe(true);
    }
  });
});
