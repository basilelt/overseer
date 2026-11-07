import { describe, expect, it } from 'vitest';
import composer from './Composer.svelte?raw';

describe('composer height for toasts', () => {
  it('reports its height on every layout, not only on the phone', () => {
    const start = composer.indexOf('reportComposerHeight(key');
    expect(start).toBeGreaterThan(-1);
    const effect = composer.slice(composer.lastIndexOf('$effect(', start), composer.indexOf('$effect(', start));
    expect(effect).not.toMatch(/phone/);
    expect(effect).toMatch(/new ResizeObserver/);
    expect(effect).toMatch(/clearComposerHeight\(key\)/);
  });
});

describe('composer textarea after its pane is widened', () => {
  it('recomputes its height from its content when its width changes', () => {
    const start = composer.indexOf('let width = el.clientWidth');
    expect(start).toBeGreaterThan(-1);
    const effect = composer.slice(composer.lastIndexOf('$effect(', start), composer.indexOf('function insert('));
    expect(effect).toMatch(/if \(el\.clientWidth === width\) return;/);
    expect(effect).toMatch(/sized = \{ length: Infinity, lines: Infinity \}/);
    expect(effect).toMatch(/autosize\(\)/);
    expect(effect).toMatch(/ro\.observe\(el\)/);
  });

  it('shrinks to auto before measuring when the size mark says the text got smaller', () => {
    expect(composer).toMatch(/const shrink = now\.length < sized\.length \|\| now\.lines < sized\.lines;/);
    expect(composer).toMatch(/if \(shrink\) el\.style\.height = 'auto';/);
  });
});
