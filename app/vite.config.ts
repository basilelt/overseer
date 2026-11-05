import { defineConfig, type Plugin } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

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
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  build: {
    target: 'es2022',
    chunkSizeWarningLimit: 900,
    reportCompressedSize: true,
    rolldownOptions: {
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
    include: ['src/**/*.test.ts'],
    css: { include: [/\.css\?raw$/] },
    coverage: {
      provider: 'v8',
      include: ['src/lib/**/*.ts'],
      exclude: ['src/**/*.test.ts', 'src/**/types.ts', 'src/**/strings.ts', 'src/**/mock*.ts', 'src/lib/i18n/*.ts'],
      reporter: ['text-summary', 'json-summary'],
      thresholds: { lines: 93, functions: 88, branches: 89, statements: 92 },
    },
  },
});
