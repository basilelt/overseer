import { createRawSnippet } from 'svelte';
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import WorkspaceFrame from './WorkspaceFrame.svelte';
import componentSrc from './WorkspaceFrame.svelte?raw';
import css from '../../../app.css?raw';
import { declOf } from '../../../test/css';
import spaceBlockSrc from './SpaceBlock.svelte?raw';

const rule = (src: string, sel: string, prop: string) => declOf(src, sel, prop) ?? '';
const box = (value: string) => {
  const v = value.trim().split(/\s+/).map(parseFloat);
  const [t, rt = t, b = t, l = rt] = v;
  return { top: t, right: rt, bottom: b, left: l };
};
const border = (value: string) => parseFloat(value);

const snip = (s: string) => createRawSnippet(() => ({ render: () => `<i>${s}</i>` }));

describe('WorkspaceFrame', () => {
  it('draws the header line above the body', () => {
    const h = render(WorkspaceFrame, { props: { header: snip('HEAD'), children: snip('BODY') } }).body;
    expect(h.indexOf('HEAD')).toBeGreaterThanOrEqual(0);
    expect(h.indexOf('HEAD')).toBeLessThan(h.indexOf('BODY'));
  });

  it('renders the right-side header content when given', () => {
    const h = render(WorkspaceFrame, { props: { header: snip('HEAD'), actions: snip('ACT'), children: snip('BODY') } }).body;
    expect(h.indexOf('ACT')).toBeGreaterThan(h.indexOf('HEAD'));
    expect(render(WorkspaceFrame, { props: { header: snip('HEAD'), children: snip('BODY') } }).body).not.toContain('ACT');
  });

  it('CSS guard: takes its look from theme tokens and rounds the frame', () => {
    expect(rule(componentSrc, '.frame', 'border')).toContain('var(--ws-frame-border)');
    expect(rule(componentSrc, '.frame', 'background')).toBe('var(--ws-frame-bg)');
    expect(rule(componentSrc, '.frame', 'border-radius')).toBe('10px');
  });

  it('CSS guard: defines the frame and row tokens for both themes', () => {
    const dark = ":root[data-theme='dark']";
    expect(declOf(css, dark, '--ws-frame-border')).toBe('#4a4a55');
    expect(declOf(css, dark, '--ws-frame-bg')).toBe('#1e1e24');
    expect(declOf(css, dark, '--session-row-bg')).toBe('#111114');
    for (const token of ['--ws-frame-border', '--ws-frame-bg', '--session-row-bg']) expect(declOf(css, ':root', token), token).toMatch(/\S/);
  });

  it('CSS guard: keeps a workspace header where it sat while folded, so opening it does not shift it', () => {
    const head = box(rule(componentSrc, '.head', 'padding'));
    const fm = box(rule(componentSrc, '.frame', 'margin'));
    const sm = box(rule(spaceBlockSrc, '.solo', 'margin'));
    const sp = box(rule(spaceBlockSrc, '.solo', 'padding'));
    const bw = border(rule(componentSrc, '.frame', 'border'));
    expect(sm.left + sp.left).toBe(fm.left + bw + head.left);
    expect(sm.right + sp.right).toBe(fm.right + bw + head.right);
  });

  it('CSS guard: insets the header and the rows by the same margin on every side', () => {
    const head = box(rule(componentSrc, '.head', 'padding'));
    const body = box(rule(componentSrc, '.body', 'padding'));
    expect(new Set([head.top, head.right, head.bottom, head.left]).size).toBe(1);
    expect(body).toEqual({ top: 0, right: head.left, bottom: head.left, left: head.left });
  });
});
