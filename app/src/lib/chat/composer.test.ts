import { describe, expect, it } from 'vitest';
import { applySlash, commandsFor, enterAction, lineCount, sendRequest, sendResult, slashMatches, slashQuery, startMode } from './composer';

const key = (k: string, o: Partial<{ shiftKey: boolean; ctrlKey: boolean; metaKey: boolean; isComposing: boolean }> = {}) => ({ key: k, shiftKey: false, ctrlKey: false, metaKey: false, ...o });

describe('composer routing', () => {
  it('sends only when idle first, then queue or send now', () => {
    expect(sendRequest('  hi ', 'ln_1', 'idle')).toEqual({ text: 'hi', lineIds: ['ln_1'], onlyIdle: true, queue: false });
    expect(sendRequest('hi', 'ln_1', 'queue')).toEqual({ text: 'hi', lineIds: ['ln_1'], onlyIdle: true, queue: true });
    expect(sendRequest('hi', 'ln_1', 'now')).toEqual({ text: 'hi', lineIds: ['ln_1'], onlyIdle: false });
    expect(sendRequest('   ', 'ln_1', 'idle')).toBeNull();
  });

  it('turns a skipped delivery into a busy choice', () => {
    expect(sendResult([{ target: 'line:a', result: 'sent' }])).toEqual({ kind: 'sent' });
    expect(sendResult([{ target: 'line:a', result: 'queued' }])).toEqual({ kind: 'queued' });
    expect(sendResult([{ target: 'line:a', result: 'skipped', reason: 'busy' }])).toEqual({ kind: 'busy', reason: 'busy' });
    expect(sendResult([{ target: 'line:a', result: 'refused', reason: 'not running' }])).toEqual({ kind: 'failed', reason: 'not running' });
    expect(sendResult(undefined)).toEqual({ kind: 'failed', reason: 'no target' });
  });

  it('asks up front when the line is known busy', () => {
    expect(startMode({ busy: true })).toBe('ask');
    expect(startMode({ needsInput: true })).toBe('ask');
    expect(startMode({ busy: false })).toBe('idle');
    expect(startMode(undefined)).toBe('idle');
  });

  it('maps Enter per layout', () => {
    expect(enterAction(key('Enter'), false)).toBe('send');
    expect(enterAction(key('Enter', { shiftKey: true }), false)).toBeNull();
    expect(enterAction(key('Enter'), true)).toBeNull();
    expect(enterAction(key('Enter', { ctrlKey: true }), true)).toBe('send');
    expect(enterAction(key('Enter', { isComposing: true }), false)).toBeNull();
    expect(enterAction(key('a'), false)).toBeNull();
  });
});

describe('slash commands', () => {
  const cmds = commandsFor('claude', [{ name: '/review-pr', description: 'Review', source: 'user' }]);

  it('detects a command being typed at the start', () => {
    expect(slashQuery('/com', 4)).toBe('com');
    expect(slashQuery('/', 1)).toBe('');
    expect(slashQuery('/compact now', 12)).toBeNull();
    expect(slashQuery('hello /x', 8)).toBeNull();
  });

  it('ranks custom and builtin commands', () => {
    expect(slashMatches('comp', cmds)[0].name).toBe('/compact');
    expect(slashMatches('rev', cmds).map((c) => c.name)).toEqual(['/review', '/review-pr']);
    expect(commandsFor('codex', []).some((c) => c.name === '/approvals')).toBe(true);
    const vibe = commandsFor('vibe', [{ name: '/review', description: 'skill', source: 'user' }]).map((c) => c.name);
    expect(vibe).toEqual(expect.arrayContaining(['/review', '/compact', '/rewind', '/branch']));
    expect(vibe).not.toContain('/approvals');
    expect(commandsFor('claude', [{ name: '/clear', description: 'mine', source: 'project' }]).filter((c) => c.name === '/clear')).toHaveLength(1);
  });

  it('replaces the typed token with the command', () => {
    expect(applySlash('/com', { name: '/compact', description: '', source: 'builtin' })).toBe('/compact ');
    expect(applySlash('/re focus on tests', { name: '/review', description: '', source: 'builtin' })).toBe('/review focus on tests');
  });
});

describe('lineCount', () => {
  it('counts lines so the composer only re-measures when it can shrink', () => {
    expect(lineCount('')).toBe(1);
    expect(lineCount('a')).toBe(1);
    expect(lineCount('a\nb\n')).toBe(3);
  });
});
