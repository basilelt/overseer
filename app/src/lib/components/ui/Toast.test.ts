import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { app } from '../../state.svelte';
import { holdWebview } from '../../popup.svelte';
import Toast from './Toast.svelte';
import { declOf } from '../../../test/css';
import toastSrc from './Toast.svelte?raw';

describe('CSS guard: toast position', () => {
  it('clears the composer on every layout', () => {
    expect(declOf(toastSrc, '.toast', 'bottom')).toBe('calc(16px + var(--composer-h, 0px))');
  });
});

describe('toast over a native webview', () => {
  afterEach(() => {
    app.toast = null;
  });

  const body = () => render(Toast, { props: {} }).body;

  it('sits at the bottom with no webview on screen', () => {
    app.toast = { text: 'saved', kind: 'info' };
    expect(body()).not.toContain('top');
  });

  it('moves to the top while a native webview is on screen, and back once it is gone', () => {
    app.toast = { text: 'saved', kind: 'info' };
    const release = holdWebview();
    expect(body()).toMatch(/class="toast[^"]*\btop\b/);
    release();
    expect(body()).not.toMatch(/class="toast[^"]*\btop\b/);
  });

  it('clears the bottom offset of every layout when it moves to the top', () => {
    expect(declOf(toastSrc, '.toast.top', 'bottom')).toBe('auto');
    expect(declOf(toastSrc, '.toast.top', 'top')).toBe('calc(8px + var(--safe-top, 0px))');
  });
});
