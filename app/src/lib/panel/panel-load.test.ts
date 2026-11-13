import { describe, expect, it, vi } from 'vitest';
import { loadPanel } from './panel-load';

const loaders = () => ({ activity: vi.fn(), diff: vi.fn(), related: vi.fn() });

describe('loadPanel', () => {
  it('loads activity, diff and related for a line in a git repository', () => {
    const l = loaders();
    loadPanel('l1', true, l);
    expect(l.activity).toHaveBeenCalledExactlyOnceWith('l1');
    expect(l.diff).toHaveBeenCalledExactlyOnceWith('l1');
    expect(l.related).toHaveBeenCalledExactlyOnceWith('l1');
  });

  it('loads activity but not diff or related for a line outside a git repository', () => {
    const l = loaders();
    loadPanel('l1', false, l);
    expect(l.activity).toHaveBeenCalledExactlyOnceWith('l1');
    expect(l.diff).not.toHaveBeenCalled();
    expect(l.related).not.toHaveBeenCalled();
  });
});
