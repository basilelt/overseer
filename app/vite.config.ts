import { defineConfig, type Plugin } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { selfUpdate } from './src/lib/mobile/self-update';

// ghostty-web inlines its WASM as a base64 data URL inside Ghostty.load(); terminal.ts loads the
// emitted ghostty-vt.wasm asset with Ghostty.load(url) instead, so that literal is dead weight.
// The other patches remove document listeners that ghostty-web never detaches, which kept
// every disposed terminal alive.
const PATCHES: [string | RegExp, string][] = [
  [/data:application\/wasm;base64,[A-Za-z0-9+/=]+/, 'data:application/wasm;base64,'],
  [
    'this.isOpen && typeof document < "u" && document.removeEventListener("mouseup", this.handleMouseUp)',
    'typeof document < "u" && document.removeEventListener("mouseup", this.handleMouseUp)',
  ],
  [/document\.addEventListener\("mousedown", \((\w+)\) => \{/, 'document.addEventListener("mousedown", this.boundDocumentMouseDownHandler = ($1) => {'],
  [
    'this.selectionChangedEmitter.dispose(), this.stopAutoScroll()',
    'this.selectionChangedEmitter.dispose(), this.stopAutoScroll(), this.boundDocumentMouseDownHandler && (document.removeEventListener("mousedown", this.boundDocumentMouseDownHandler), this.boundDocumentMouseDownHandler = null)',
  ],
  // Copy joins soft-wrapped rows instead of breaking a long command at the right margin. Only rows
  // on screen report wrapping; scrollback rows always read as unwrapped in this ghostty-web build.
  [
    /(\w) >= 0 \? (\w) = \2\.substring\(0, \1\) : \2 = "", (\w) \+= \2, (\w) < (\w) && \(\3 \+= `\n`\);/,
    'const sb = this.wasmTerm.getScrollbackLength(), wraps = $4 < $5 && $4 + 1 >= sb && this.wasmTerm.isRowWrapped($4 + 1 - sb); wraps || ($1 >= 0 ? $2 = $2.substring(0, $1) : $2 = ""), $3 += $2, $4 < $5 && !wraps && ($3 += `\n`);',
  ],
  [
    'clearSelection() {\n    if (!this.hasSelection())\n      return;\n    const A = this.normalizeSelection();\n    if (A)\n      for (let Q = A.startRow; Q <= A.endRow; Q++)\n        this.dirtySelectionRows.add(Q);',
    'clearSelection(A) {\n    if (typeof A == "string" && /^\\x1B\\[(?:<|M)/.test(A))\n      return;\n    if (!this.hasSelection())\n      return;\n    const B = this.normalizeSelection();\n    if (B)\n      for (let Q = B.startRow; Q <= B.endRow; Q++)\n        this.dirtySelectionRows.add(Q);',
  ],
  [
    '(w = this.selectionManager) == null || w.clearSelection(), this.awaitingEcho = !0, this.dataEmitter.fire(o)',
    '(w = this.selectionManager) == null || w.clearSelection(o), this.awaitingEcho = !0, this.dataEmitter.fire(o)',
  ],
  [
    'this.renderer.setSelectionManager(this.selectionManager), this.selectionManager.onSelectionChange',
    'this.renderer.setSelectionManager(this.selectionManager), A.__overseerSel = this.selectionManager, this.selectionManager.onSelectionChange',
  ],
  [
    'const Q = ((D = this.getModeCallback) == null ? void 0 : D.call(this, 1002)) ?? !1, g = ((o = this.getModeCallback) == null ? void 0 : o.call(this, 1003)) ?? !1;',
    'const S = this.container.__overseerSel;\n    if (S && S.isSelecting && S.dragThresholdMet)\n      return;\n    const Q = ((D = this.getModeCallback) == null ? void 0 : D.call(this, 1002)) ?? !1, g = ((o = this.getModeCallback) == null ? void 0 : o.call(this, 1003)) ?? !1;',
  ],
  // Shift+drag with mouse tracking on goes to the program; the shift bit is dropped from reports because tmux and vim name the event S-MouseDrag otherwise.
  [
    'A.addEventListener("mousedown", (Q) => {\n      if (Q.button === 0) {\n        A.parentElement && A.parentElement.focus();',
    'A.addEventListener("mousedown", (Q) => {\n      if (Q.button === 0 && !(Q.shiftKey && this.wasmTerm.hasMouseTracking())) {\n        A.parentElement && A.parentElement.focus();',
  ],
  [
    'A.shiftKey && (Q |= 4), A.metaKey && (Q |= 8)',
    'A.metaKey && (Q |= 8)',
  ],
];

export function patchGhostty(code: string): string {
  for (const [from, to] of PATCHES) {
    const hits = typeof from === 'string' ? code.split(from).length - 1 : (code.match(new RegExp(from.source, 'g')) ?? []).length;
    if (hits !== 1) throw new Error(`ghostty-web patch matched ${hits} times, expected 1: ${from}`);
    code = code.replace(from, to);
  }
  return code;
}

function ghosttyPatches(): Plugin {
  return {
    name: 'ghostty-patches',
    transform(code, id) {
      const [file, query = ''] = id.split('?');
      if (!/ghostty-web[\\/]dist[\\/]ghostty-web\.js$/.test(file) || /(^|&)raw\b/.test(query)) return;
      return { code: patchGhostty(code), map: null };
    },
  };
}

export default defineConfig({
  plugins: [svelte(), ghosttyPatches()],
  clearScreen: false,
  optimizeDeps: { exclude: ['ghostty-web'] },
  define: { __SELF_UPDATE__: JSON.stringify(selfUpdate()) },
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  build: {
    target: 'es2022',
    chunkSizeWarningLimit: 900,
    reportCompressedSize: true,
    rolldownOptions: {
      optimization: { inlineConst: true },
      output: {
        codeSplitting: {
          // Keeps rolldown's CJS interop runtime out of the lazy hljs chunk, which the entry would otherwise modulepreload.
          groups: [{ name: 'hljs-core', test: /highlight\.js\/lib\/core/ }],
        },
      },
    },
  },
  test: {
    environment: 'node',
    unstubGlobals: true,
    unstubEnvs: true,
    restoreMocks: true,
    setupFiles: ['src/test/pin-locale.ts', 'src/test/pin-platform.ts'],
    include: ['src/**/*.test.ts'],
    css: { include: [/\.css\?raw$/] },
    coverage: {
      provider: 'v8',
      include: ['src/lib/**/*.ts'],
      exclude: ['src/**/*.test.ts', 'src/**/types.ts', 'src/**/strings.ts', 'src/**/mock*.ts', 'src/lib/i18n/*.ts'],
      reporter: ['text-summary', 'json-summary'],
      thresholds: { lines: 96.11, functions: 92.07, branches: 91.41, statements: 95.07 },
    },
  },
});
