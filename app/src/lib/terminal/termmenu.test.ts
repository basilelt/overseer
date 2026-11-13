import { describe, expect, it, vi } from 'vitest';
import { termMenuItems } from './termmenu';

const hint = (id: 'copy' | 'paste') => (id === 'copy' ? 'Ctrl+Shift+C' : 'Ctrl+Shift+V');

describe('termMenuItems', () => {
  it('offers Copy and Paste with their shortcuts', () => {
    const items = termMenuItems({ hasSelection: true, copy: vi.fn(), paste: vi.fn(), hint });
    expect(items.map((i) => [i.label, i.hint])).toEqual([
      ['Copy', 'Ctrl+Shift+C'],
      ['Paste', 'Ctrl+Shift+V'],
    ]);
  });

  it('disables Copy without a selection and keeps Paste enabled', () => {
    const [copy, paste] = termMenuItems({ hasSelection: false, copy: vi.fn(), paste: vi.fn(), hint });
    expect(copy.disabled).toBe(true);
    expect(paste.disabled).toBeFalsy();
  });

  it('runs the matching action', () => {
    const copy = vi.fn();
    const paste = vi.fn();
    const items = termMenuItems({ hasSelection: true, copy, paste, hint });
    items[0].run?.();
    items[1].run?.();
    expect(copy).toHaveBeenCalledOnce();
    expect(paste).toHaveBeenCalledOnce();
  });
});
