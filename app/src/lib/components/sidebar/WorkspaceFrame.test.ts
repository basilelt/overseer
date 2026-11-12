import { createRawSnippet } from 'svelte';
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import WorkspaceFrame from './WorkspaceFrame.svelte';
import componentSrc from './WorkspaceFrame.svelte?raw';
import css from '../../../app.css?raw';
import spaceBlockSrc from './SpaceBlock.svelte?raw';

const rule = (src: string, sel: string) => {
  const m = new RegExp(`\\n  ${sel.replace('.', '\\.')} \\{([^}]*)\\}`).exec(src.slice(src.indexOf('<style>')));
  if (!m) throw new Error(`no rule for ${sel}`);
  return m[1];
};
const box = (r: string, prop: string) => {
  const v = new RegExp(`(?:^|\\s)${prop}:\\s*([^;]+);`).exec(r)?.[1].trim().split(/\s+/).map(parseFloat) ?? [0];
  const [t, rt = t, b = t, l = rt] = v;
  return { top: t, right: rt, bottom: b, left: l };
};
const border = (r: string) => parseFloat(/(?:^|\s)border:\s*(\d+)px/.exec(r)?.[1] ?? '0');

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

  it('takes its look from theme tokens, not fixed colours', () => {
    const src = componentSrc;
    expect(src).toContain('var(--ws-frame-border)');
    expect(src).toContain('var(--ws-frame-bg)');
    expect(src).toContain('border-radius: 10px');
    expect(src).not.toMatch(/#[0-9a-f]{3,6}\b|transition|animation/i);
  });

  it('defines frame tokens for both themes', () => {
        const dark = css.slice(css.indexOf(":root[data-theme='dark']"));
    expect(dark).toMatch(/--ws-frame-border: #4a4a55/);
    expect(dark).toMatch(/--ws-frame-bg: #1e1e24/);
    expect(dark).toMatch(/--session-row-bg: #111114/);
    const light = css.slice(0, css.indexOf(":root[data-theme='dark']"));
    expect(light).toMatch(/--ws-frame-border:/);
    expect(light).toMatch(/--session-row-bg:/);
  });

  it('keeps a workspace header where it sat while folded, so opening it does not shift it', () => {
    const frame = rule(componentSrc, '.frame');
    const head = box(rule(componentSrc, '.head'), 'padding');
    const solo = rule(spaceBlockSrc, '.solo');
    const fm = box(frame, 'margin');
    const sm = box(solo, 'margin');
    const sp = box(solo, 'padding');
    expect(sm.left + sp.left).toBe(fm.left + border(frame) + head.left);
    expect(sm.right + sp.right).toBe(fm.right + border(frame) + head.right);
  });

  it('insets the header and the rows by the same margin on every side', () => {
    const head = box(rule(componentSrc, '.head'), 'padding');
    const body = box(rule(componentSrc, '.body'), 'padding');
    expect(new Set([head.top, head.right, head.bottom, head.left]).size).toBe(1);
    expect(body).toEqual({ top: 0, right: head.left, bottom: head.left, left: head.left });
  });
});
