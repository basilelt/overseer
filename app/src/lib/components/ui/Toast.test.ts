import { describe, expect, it } from 'vitest';
import toast from './Toast.svelte?raw';

describe('toast position', () => {
  it('clears the composer on every layout', () => {
    const style = toast.slice(toast.indexOf('<style'));
    expect(style).toMatch(/\.toast \{[^}]*bottom:\s*calc\(16px \+ var\(--composer-h, 0px\)\)/);
  });
});
