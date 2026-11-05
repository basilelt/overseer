import { describe, expect, it } from 'vitest';
import dialog from './ReviewDialog.svelte?raw';
import modal from '../ui/Modal.svelte?raw';

const rule = (src: string, selector: string) => {
  const m = src.match(new RegExp(`${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{([^}]*)\\}`));
  return m ? m[1] : '';
};
const prop = (body: string, name: string) => body.match(new RegExp(`(?:^|[\\s;])${name}:\\s*([^;]+);`))?.[1].trim() ?? '';

describe('review dialog layout', () => {
  const modalVh = Number(prop(rule(modal, '.modal'), 'max-height').match(/^(\d+)vh$/)?.[1]);

  it('keeps the diff frame shorter than the modal body so the submit bar sits above the footer', () => {
    const height = prop(rule(dialog, '.frame'), 'height');
    const vh = Number(height.match(/calc\((\d+)vh\s*-\s*(\d+)px\)/)?.[1]);
    const reserve = Number(height.match(/calc\((\d+)vh\s*-\s*(\d+)px\)/)?.[2]);
    expect(height).not.toMatch(/\b62vh\b/);
    expect(vh).toBeLessThanOrEqual(modalVh);
    expect(reserve).toBeGreaterThanOrEqual(190);
  });

  it('sizes the frame for the phone sheet from its dynamic viewport height', () => {
    const phone = prop(rule(dialog, ':global(.phone) .frame'), 'height');
    expect(phone).toMatch(/calc\(90dvh\s*-\s*\d+px\)/);
  });
});
