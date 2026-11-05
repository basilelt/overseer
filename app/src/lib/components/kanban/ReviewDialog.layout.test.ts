import { describe, expect, it } from 'vitest';
import dialog from './ReviewDialog.svelte?raw';
import modal from '../ui/Modal.svelte?raw';

const rule = (src: string, selector: string) => {
  const m = src.match(new RegExp(`${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{([^}]*)\\}`));
  return m ? m[1] : '';
};
const prop = (body: string, name: string) => body.match(new RegExp(`(?:^|[\\s;])${name}:\\s*([^;]+);`))?.[1].trim() ?? '';

describe('review dialog layout', () => {
  it('opts the modal into fill so the body is a flex column with a definite height', () => {
    expect(dialog).toMatch(/<Modal[^>]*\sfill>/);
    expect(prop(rule(modal, '.modal.fill'), 'height')).toBe('76vh');
    expect(prop(rule(modal, ':global(.phone) .modal.fill'), 'height')).toBe('90dvh');
    const content = rule(modal, '.content.fill');
    expect(prop(content, 'flex')).toBe('1 1 0');
    expect(prop(content, 'display')).toBe('flex');
    expect(prop(content, 'flex-direction')).toBe('column');
    expect(prop(content, 'min-height')).toBe('0');
  });

  it('lets the diff frame take the remaining space instead of a viewport subtraction', () => {
    const frame = rule(dialog, '.frame');
    expect(prop(frame, 'flex')).toMatch(/^1 1 0/);
    expect(prop(frame, 'min-height')).toBe('320px');
    expect(prop(frame, 'height')).toBe('');
    expect(dialog).not.toMatch(/calc\(\d+d?vh\s*-/);
  });

  it('keeps the dialog header from shrinking', () => {
    expect(prop(rule(dialog, '.head'), 'flex')).toBe('none');
  });
});
