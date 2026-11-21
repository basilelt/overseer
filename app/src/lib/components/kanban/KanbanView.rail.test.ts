import { describe, expect, it } from 'vitest';
import source from './KanbanView.svelte?raw';

describe('KanbanView collapsed rails during a drag', () => {
  it('opens a collapsed rail only when the pointer rests on it, so crossing it does not move the columns after it', () => {
    expect(source).toMatch(/const RAIL_DWELL = \d+;/);
    expect(source).toMatch(/function armRail\(on: TaskStatus \| null\) \{\s*clearTimeout\(railTimer\);\s*if \(railOpen && railOpen !== on\) railOpen = null;\s*if \(on && collapsed\[on\] && railOpen !== on\) railTimer = setTimeout\(\(\) => \(railOpen = on\), RAIL_DWELL\);/);
    expect(source).toMatch(/armRail\(at\.target\);/);
    expect(source).toMatch(/drag = null;\s*armRail\(null\);\s*suppressClick = true;/);
    expect(source).toContain('collapsed={!!collapsed[col.status] && railOpen !== col.status && picked?.target !== col.status}');
  });

  it('still lets the user drop on a collapsed rail', () => {
    expect(source).toContain('target={dropOn === col.status && origin !== col.status}');
    expect(source).not.toContain('collapsed={!!collapsed[col.status] && dropOn !== col.status}');
  });
});
