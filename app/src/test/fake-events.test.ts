import { describe, expect, it, vi } from 'vitest';
import { eventTarget } from './fake-events';

describe('eventTarget', () => {
  it('delivers an event to every handler of its type and to none of another type', () => {
    const t = eventTarget();
    const a = vi.fn();
    const b = vi.fn();
    const other = vi.fn();
    t.addEventListener('x', a);
    t.addEventListener('x', b);
    t.addEventListener('y', other);
    t.fire('x', { n: 1 });
    expect(a).toHaveBeenCalledWith({ n: 1 });
    expect(b).toHaveBeenCalledWith({ n: 1 });
    expect(other).not.toHaveBeenCalled();
  });

  it('stops delivering to a removed handler and counts what is left', () => {
    const t = eventTarget();
    const a = vi.fn();
    t.addEventListener('x', a);
    t.addEventListener('y', a);
    expect(t.count()).toBe(2);
    t.removeEventListener('x', a);
    t.fire('x');
    expect(a).not.toHaveBeenCalled();
    expect(t.has('x')).toBe(false);
    expect(t.has('y')).toBe(true);
    t.clear();
    expect(t.count()).toBe(0);
  });

  it('ignores an event nobody listens to', () => {
    expect(() => eventTarget().fire('none')).not.toThrow();
  });
});
