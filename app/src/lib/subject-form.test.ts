import { describe, expect, it } from 'vitest';
import { relativeTo, subjectBody } from './subject-form';

const parent = { id: 'w1', path: '/home/u/endurance' };

describe('subjectBody', () => {
  it('nests a relative path under the parent and starts at its root when asked', () => {
    expect(subjectBody(parent, '20_processus/onboarding/', ' Onboarding PME ', true)).toEqual({ path: '/home/u/endurance/20_processus/onboarding', name: 'Onboarding PME', parentId: 'w1', cwd: '/home/u/endurance' });
  });

  it('starts in the subject folder and lets the daemon name it when no name is given', () => {
    expect(subjectBody(parent, './40_code/os', '', false)).toEqual({ path: '/home/u/endurance/40_code/os', parentId: 'w1' });
  });

  it('keeps an absolute or home path as typed', () => {
    expect(subjectBody(parent, '~/elsewhere', 'x', false)).toEqual({ path: '~/elsewhere', name: 'x', parentId: 'w1' });
    expect(subjectBody(parent, '/srv/x', '', true)).toEqual({ path: '/srv/x', parentId: 'w1', cwd: '/home/u/endurance' });
  });

  it('makes a subject without a folder from a name alone, and nothing from neither', () => {
    expect(subjectBody(parent, '  ', 'Archi V3', true)).toEqual({ kind: 'subject', name: 'Archi V3', parentId: 'w1' });
    expect(subjectBody(parent, '', '  ', true)).toBeNull();
  });
});

describe('relativeTo', () => {
  it('shows a picked folder relative to the parent and keeps one outside it absolute', () => {
    expect(relativeTo(parent, '/home/u/endurance/10_config/architecture')).toBe('10_config/architecture');
    expect(relativeTo(parent, '/home/u/endurance/')).toBe('');
    expect(relativeTo(parent, '/home/u/endurance-old/x')).toBe('/home/u/endurance-old/x');
    expect(relativeTo({ path: null }, '/a/b')).toBe('/a/b');
  });
});
