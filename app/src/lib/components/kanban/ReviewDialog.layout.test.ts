import { describe, expect, it } from 'vitest';
import { declOf } from '../../../test/css';
import dialog from './ReviewDialog.svelte?raw';
import modal from '../ui/Modal.svelte?raw';

describe('CSS guard: review dialog layout', () => {
  it('gives a fill modal a definite height and a flex column body that can shrink', () => {
    expect(declOf(modal, '.modal.fill', 'height')).toBe('76vh');
    expect(declOf(modal, ':global(.phone) .modal.fill', 'height')).toBe('90dvh');
    expect(declOf(modal, '.content.fill', 'flex')).toBe('1 1 0');
    expect(declOf(modal, '.content.fill', 'display')).toBe('flex');
    expect(declOf(modal, '.content.fill', 'flex-direction')).toBe('column');
    expect(declOf(modal, '.content.fill', 'min-height')).toBe('0');
  });

  it('lets the diff frame take the remaining space instead of a fixed height', () => {
    expect(declOf(dialog, '.frame', 'flex')).toMatch(/^1 1 0/);
    expect(declOf(dialog, '.frame', 'min-height')).toBe('320px');
    expect(declOf(dialog, '.frame', 'height')).toBeUndefined();
  });

  it('gives the phone frame room for the review chrome plus a readable diff', () => {
    expect(declOf(dialog, ':global(.phone) .frame', 'min-height')).toBe('470px');
  });

  it('keeps the dialog header from shrinking', () => {
    expect(declOf(dialog, '.head', 'flex')).toBe('none');
  });
});
