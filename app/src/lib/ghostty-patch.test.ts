import { describe, expect, it } from 'vitest';
import source from 'ghostty-web?raw';
import { patchGhostty } from '../../vite.config';

describe('patchGhostty', () => {
  const patched = patchGhostty(source);

  it('drops the inlined wasm', () => {
    expect(patched).not.toMatch(/data:application\/wasm;base64,[A-Za-z0-9+/=]{16}/);
    expect(patched.length).toBeLessThan(source.length);
  });

  it('removes the document mouseup listener even after isOpen is cleared', () => {
    expect(patched).toContain('typeof document < "u" && document.removeEventListener("mouseup", this.handleMouseUp)');
    expect(patched).not.toContain('this.isOpen && typeof document < "u" && document.removeEventListener("mouseup"');
  });

  it('detaches the selection mousedown listener on dispose', () => {
    expect(patched).toContain('document.addEventListener("mousedown", this.boundDocumentMouseDownHandler = ');
    expect(patched).toContain('document.removeEventListener("mousedown", this.boundDocumentMouseDownHandler)');
  });

  it('fails loudly when an anchor is gone', () => {
    expect(() => patchGhostty('nothing to patch')).toThrow(/ghostty-web patch matched 0 times/);
  });
});
