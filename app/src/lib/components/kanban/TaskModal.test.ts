import { describe, expect, it } from 'vitest';
import source from './TaskModal.svelte?raw';

describe('task type in the add task form', () => {
  it('resolves the model and effort with the task type', () => {
    expect(source).toContain('resolveProfile(app.profiles, { explicit: profile, taskType: taskType || undefined, cwd: project })');
  });

  it('refills model and effort when the type changes unless a profile is picked', () => {
    expect(source).toMatch(/function setTaskType\(ty: TaskType \| ''\) \{\s*taskType = ty;\s*if \(!profile\) reset\(\);/);
  });

  it('takes the type of an imported ticket', () => {
    expect(source).toContain('if (fill.taskType && app.profiles?.taskTypes) setTaskType(fill.taskType);');
  });

  it('keeps only the profile field on a daemon without task types', () => {
    expect(source).toMatch(/\{#if app\.profiles\?\.taskTypes\}[\s\S]*\{:else if app\.profiles\}\s*<div class="field">\s*<span>\{t\('profile\.label'\)\}<\/span>\s*<ProfileSelect value=\{profile\} cwd=\{project\} \{agent\} onchange=\{setProfile\} \/>/);
  });

  it('sends the type and shows the profile it implies', () => {
    expect(source).toContain('...(taskType ? { taskType } : {}),');
    expect(source).toContain('<ProfileSelect value={profile} cwd={project} {taskType} {agent} onchange={setProfile} />');
  });
});
