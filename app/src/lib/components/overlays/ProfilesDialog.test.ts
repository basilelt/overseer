import { describe, expect, it } from 'vitest';
import source from './ProfilesDialog.svelte?raw';

describe('agent sections of the profiles dialog', () => {
  const tag = source.match(/<details class="agent".*/)?.[0] ?? '';

  it('keeps a section the user opened open when a field in it changes', () => {
    expect(tag).toMatch(/open=\{expanded\[a\.id\] \?\?/);
    expect(tag).toMatch(/ontoggle=\{\(e\) => \(expanded\[a\.id\] = e\.currentTarget\.open\)\}/);
  });

  it('still opens the only section by default', () => {
    expect(tag).toMatch(/\?\? agents\.length === 1\}/);
  });

  it('offers native model tiers for Antigravity named subagents', () => {
    expect(source).toContain("{#if d.agent === 'agy'}");
    expect(source).toContain("<option value=\"inherit\">{t('profile.inherits')}</option>");
    expect(source).toContain("<option value=\"flash\">{t('profile.subagentTier.flash')}</option>");
    expect(source).not.toContain('flash_lite');
    expect(source).toContain('setSub(a.id, d.name, { model: e.currentTarget.value })');
  });

  it('maps each task type to a profile and saves on change', () => {
    expect(source).toContain('{#each TASK_TYPES as ty (ty)}');
    expect(source).toContain('onchange={(e) => setType(ty, e.currentTarget.value)}');
    expect(source).toMatch(/if \(id\) types\[ty\] = id;\s*else delete types\[ty\];/);
  });

  it('drops task type rules that point at a deleted profile on save', () => {
    expect(source).toContain('types: Object.fromEntries(Object.entries(store.types ?? {}).filter(([, id]) => known(id))),');
  });

  it('wraps profile actions on phone so Delete stays visible', () => {
    expect(source).toMatch(/:global\(\.phone\) \.head\s*\{\s*flex-wrap:\s*wrap;/);
    expect(source).toMatch(/:global\(\.phone\) \.head \.name\s*\{\s*width:\s*100%;/);
    expect(source).toMatch(/:global\(\.phone\) \.head \.spacer\s*\{\s*display:\s*none;/);
  });
});
