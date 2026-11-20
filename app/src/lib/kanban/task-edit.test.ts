import { describe, expect, it } from 'vitest';
import type { Effort, Task } from '../types';
import { editPatch, type EditFields } from './task-edit';

const task = (over: Partial<Task> = {}) =>
  ({
    id: 't1',
    number: 1,
    title: 'Fix login',
    description: 'users cannot log in',
    status: 'pending',
    project: '/repo',
    agent: 'claude',
    model: 'opus',
    effort: 'medium',
    rank: 1,
    station: '',
    ticketKey: 'PROJ-1',
    taskType: 'fix',
    profile: 'balanced',
    createdAt: '2026-11-05T10:00:00Z',
    updatedAt: '2026-11-05T10:00:00Z',
    ...over,
  }) as Task;

const fields = (over: Partial<EditFields> = {}): EditFields => ({
  title: 'Fix login',
  description: 'users cannot log in',
  agent: 'claude',
  model: 'opus',
  effort: 'medium' as Effort,
  hasModel: true,
  effortOptions: ['low', 'medium', 'high'],
  ticketKey: 'PROJ-1',
  taskType: 'fix',
  profile: 'balanced',
  ...over,
});

describe('editPatch', () => {
  it('returns null when nothing changed', () => {
    expect(editPatch(task(), fields())).toBeNull();
  });

  it('patches the title trimmed, and ignores an empty title', () => {
    expect(editPatch(task(), fields({ title: '  Fix the login  ' }))).toEqual({ title: 'Fix the login' });
    expect(editPatch(task(), fields({ title: '   ' }))).toBeNull();
  });

  it('patches the description verbatim, empty included', () => {
    expect(editPatch(task(), fields({ description: '' }))).toEqual({ description: '' });
  });

  it('patches the agent (the AI CLI) when it changed', () => {
    expect(editPatch(task(), fields({ agent: 'codex' }))).toEqual({ agent: 'codex' });
  });

  it('patches the model, and clears it when the new agent has no models', () => {
    expect(editPatch(task(), fields({ model: 'haiku' }))).toEqual({ model: 'haiku' });
    expect(editPatch(task(), fields({ hasModel: false }))).toEqual({ model: '' });
    expect(editPatch(task({ model: '' }), fields({ hasModel: false }))).toBeNull();
  });

  it('patches the effort only when the new agent still offers it, else clears it', () => {
    expect(editPatch(task(), fields({ effort: 'high' }))).toEqual({ effort: 'high' });
    expect(editPatch(task(), fields({ effort: 'high', effortOptions: [] }))).toEqual({ effort: '' });
    expect(editPatch(task({ effort: null as never }), fields({ effortOptions: [] }))).toBeNull();
  });

  it('patches the ticket key, and clears it with null when emptied', () => {
    expect(editPatch(task(), fields({ ticketKey: ' PROJ-2 ' }))).toEqual({ ticketKey: 'PROJ-2' });
    expect(editPatch(task(), fields({ ticketKey: '' }))).toEqual({ ticketKey: null });
    expect(editPatch(task({ ticketKey: null }), fields({ ticketKey: '' }))).toBeNull();
  });

  it('patches the task type, clearing it with null when unset', () => {
    expect(editPatch(task(), fields({ taskType: 'feature' }))).toEqual({ taskType: 'feature' });
    expect(editPatch(task(), fields({ taskType: '' }))).toEqual({ taskType: null });
    expect(editPatch(task({ taskType: null }), fields({ taskType: '' }))).toBeNull();
  });

  it("patches the profile, mapping '' back to inherit (null) and keeping 'none'", () => {
    expect(editPatch(task(), fields({ profile: 'fast' }))).toEqual({ profile: 'fast' });
    expect(editPatch(task(), fields({ profile: '' }))).toEqual({ profile: null });
    expect(editPatch(task(), fields({ profile: 'none' }))).toEqual({ profile: 'none' });
    expect(editPatch(task({ profile: null }), fields({ profile: '' }))).toBeNull();
  });

  it('collects every changed field in one patch', () => {
    expect(editPatch(task(), fields({ title: 'New', description: 'd2', agent: 'gemini', model: '', ticketKey: '', taskType: '', profile: '' }))).toEqual({
      title: 'New',
      description: 'd2',
      agent: 'gemini',
      model: '',
      ticketKey: null,
      taskType: null,
      profile: null,
    });
  });
});
