import { describe, expect, it } from 'vitest';
import { chatCapable, defaultMode, tabMode, withMode } from './viewmode';

const claude = { kind: 'claude', lineId: 'ln_1', key: 'line:ln_1' };

describe('chat view mode', () => {
  it('defaults to chat on phones for agents with a transcript parser', () => {
    expect(defaultMode('phone', claude)).toBe('chat');
    expect(defaultMode('phone', { kind: 'codex', lineId: 'x' })).toBe('chat');
    expect(defaultMode('phone', { kind: 'vibe', lineId: 'x' })).toBe('chat');
    expect(defaultMode('desktop', claude)).toBe('terminal');
    expect(defaultMode('phone', { kind: 'aider', lineId: 'x' })).toBe('terminal');
    expect(defaultMode('phone', { kind: 'shell' })).toBe('terminal');
    expect(defaultMode('phone', { kind: 'claude' })).toBe('terminal');
    expect(chatCapable(undefined)).toBe(false);
  });

  it('remembers a choice per layout and line', () => {
    let saved = withMode({}, 'desktop', claude, 'chat');
    expect(tabMode(saved, 'desktop', claude)).toBe('chat');
    expect(tabMode(saved, 'desktop', { ...claude, key: 'term:t1' })).toBe('chat');
    expect(tabMode(saved, 'phone', claude)).toBe('chat');
    saved = withMode(saved, 'phone', claude, 'terminal');
    expect(tabMode(saved, 'phone', claude)).toBe('terminal');
    saved = withMode(saved, 'desktop', claude, 'terminal');
    expect(saved).toEqual({ 'phone:line:ln_1': 'terminal' });
  });

  it('forces terminal for tabs that cannot chat', () => {
    expect(tabMode({ 'desktop:line:x': 'chat' }, 'desktop', { kind: 'aider', lineId: 'x' })).toBe('terminal');
    expect(tabMode({}, 'phone', undefined)).toBe('terminal');
  });
});
