import { loadGhostty, termTheme } from '../src/lib/terminal';

const q = new URLSearchParams(location.search);
const stats = { write: 0, render: 0, paints: 0, bytes: 0, wall: 0, done: false };
(window as unknown as { stats: typeof stats }).stats = stats;

function line(i: number): string {
  const words = Array.from({ length: 12 }, (_, k) => `\x1b[${k % 3 ? 0 : 1};${30 + ((i + k) % 8)}mword${(i * 7 + k) % 997}\x1b[0m`);
  return `\x1b[${31 + (i % 7)}m${String(i).padStart(6)}\x1b[0m │ ${words.join(' ')} \x1b[2m(${i % 13} ms)\x1b[0m\r\n`;
}

(async () => {
  const { mod, ghostty } = await loadGhostty();
  const term = new mod.Terminal({ ghostty, fontSize: 13, fontFamily: 'monospace', theme: termTheme(q.get('theme') === 'light' ? 'light' : 'dark', {}), scrollback: 4_000_000 });
  const fit = new mod.FitAddon();
  term.loadAddon(fit);
  term.open(document.getElementById('t')!);
  fit.fit();
  (window as unknown as { term: typeof term }).term = term;
  const r = (term as unknown as { renderer: { render: (...a: unknown[]) => void } }).renderer;
  const render = r.render.bind(r);
  r.render = (...a: unknown[]) => {
    const t0 = performance.now();
    render(...a);
    stats.render += performance.now() - t0;
    stats.paints++;
  };
  const total = Number(q.get('lines') ?? 20000);
  const start = performance.now();
  let i = 0;
  const pump = () => {
    let chunk = '';
    for (let k = 0; k < 40 && i < total; k++) chunk += line(i++);
    const t0 = performance.now();
    term.write(chunk);
    stats.write += performance.now() - t0;
    stats.bytes += chunk.length;
    if (i < total) requestAnimationFrame(pump);
    else requestAnimationFrame(() => requestAnimationFrame(() => ((stats.wall = performance.now() - start), (stats.done = true))));
  };
  requestAnimationFrame(pump);
})();
