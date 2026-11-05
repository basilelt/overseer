export const PERF_TERMS = 5;
export const STREAM_EVERY_MS = 50;
export const STREAM_LINES = 10;

export function streamBlock(seq: number): string {
  let out = '';
  for (let i = 0; i < STREAM_LINES; i++) {
    const n = seq * STREAM_LINES + i;
    out += `\x1b[2m${String(n).padStart(7, '0')}\x1b[0m \x1b[3${(n % 6) + 1}mbuild\x1b[0m compiling crate_${n % 97} v0.${n % 13}.0 (/work/src/module_${n % 31}.rs) ... ok\r\n`;
  }
  return out;
}
