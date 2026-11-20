import { describe, expect, it } from 'vitest';
import source from 'ghostty-web?raw';
import config, { patchGhostty } from '../../../vite.config';

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

  it('lets a drag select locally in a program that reads the mouse', () => {
    // clearSelection takes the input chunk and ignores mouse reports (SGR and X10)
    expect(patched).toContain('clearSelection(A) {');
    expect(patched).toContain('if (typeof A == "string" && /^\\x1B\\[(?:<|M)/.test(A))');
    expect(patched).not.toContain('clearSelection() {\n    if (!this.hasSelection())');
    // the input callback passes the chunk through, so a mouse report no longer wipes the selection
    expect(patched).toContain('w.clearSelection(o), this.awaitingEcho = !0');
    // the selection manager is reachable from the input handler through the terminal element
    expect(patched).toContain('A.__overseerSel = this.selectionManager');
    // drag motion is not forwarded while a selection drag is in progress
    expect(patched).toContain('const S = this.container.__overseerSel;');
    expect(patched).toContain('if (S && S.isSelecting && S.dragThresholdMet)');
  });

  it('keeps ghostty-web out of the dev server pre-bundle so the desktop dev app runs the patched build', () => {
    expect((config as { optimizeDeps?: { exclude?: string[] } }).optimizeDeps?.exclude).toContain('ghostty-web');
  });

  it('fails loudly when an anchor is gone', () => {
    expect(() => patchGhostty('nothing to patch')).toThrow(/ghostty-web patch matched 0 times/);
  });
});
