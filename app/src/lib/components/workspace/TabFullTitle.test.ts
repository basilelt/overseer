import { describe, expect, it } from 'vitest';
import overlay from './TabFullTitle.svelte?raw';
import bar from './TabBar.svelte?raw';

function rule(source: string, selector: string): string {
  const style = source.slice(source.indexOf('<style'));
  const at = style.search(new RegExp(`\\n\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{`));
  expect(at).toBeGreaterThan(-1);
  return style.slice(at, style.indexOf('}', at));
}

describe('full title overlay of a cut tab', () => {
  it('has no close button of its own, so the close button does not move on hover', () => {
    const markup = overlay.slice(overlay.indexOf('</script>'), overlay.indexOf('<style'));
    expect(markup).not.toMatch(/<button/);
  });

  it('ends where the cut title ends and grows leftwards, clear of the close button', () => {
    const full = rule(overlay, '.full');
    expect(full).toMatch(/^\s*right:\s*0;/m);
    expect(full).not.toMatch(/^\s*left:/m);
  });

  it('leaves the close button in the normal stacking order', () => {
    expect(rule(bar, '.x')).not.toMatch(/z-index/);
  });
});
