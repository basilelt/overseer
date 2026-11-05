import type { Row } from './layout';
import { MAX_LANES, PALETTE_SIZE, ROW_H, laneX } from './view';

export interface Theme {
  lanes: string[];
  accent: string;
  bg: string;
}

export function readTheme(el: Element = document.documentElement): Theme {
  const css = getComputedStyle(el);
  const v = (n: string) => css.getPropertyValue(n).trim();
  return { lanes: Array.from({ length: PALETTE_SIZE }, (_, i) => v(`--lane-${i}`) || '#888'), accent: v('--accent'), bg: v('--bg') };
}

const clampLane = (l: number) => Math.min(l, MAX_LANES - 1);

export function drawGraph(
  ctx: CanvasRenderingContext2D,
  rows: readonly Row[],
  start: number,
  end: number,
  theme: Theme,
  opts: { width: number; head: number; merges: (i: number) => boolean; dimmed?: (i: number) => boolean },
) {
  const h = (end - start) * ROW_H;
  ctx.clearRect(0, 0, opts.width, h);
  ctx.lineWidth = 1.6;
  ctx.lineCap = 'round';
  const color = (c: number) => theme.lanes[((c % PALETTE_SIZE) + PALETTE_SIZE) % PALETTE_SIZE];
  for (let i = start; i < end; i++) {
    const r = rows[i];
    const y0 = (i - start) * ROW_H;
    const yc = y0 + ROW_H / 2;
    const y1 = y0 + ROW_H;
    ctx.globalAlpha = opts.dimmed?.(i) ? 0.35 : 1;
    for (const s of r.segs) {
      if (s.from >= MAX_LANES && s.to >= MAX_LANES) continue;
      const x1 = laneX(clampLane(s.from));
      const x2 = laneX(clampLane(s.to));
      ctx.strokeStyle = color(s.color);
      ctx.beginPath();
      if (s.kind === 'pass') {
        ctx.moveTo(x1, y0);
        ctx.lineTo(x1, y1);
      } else if (s.kind === 'in') {
        ctx.moveTo(x1, y0);
        if (x1 === x2) ctx.lineTo(x2, yc);
        else ctx.bezierCurveTo(x1, y0 + ROW_H * 0.35, x2, yc - ROW_H * 0.2, x2, yc);
      } else {
        ctx.moveTo(x1, yc);
        if (x1 === x2) ctx.lineTo(x2, y1);
        else ctx.bezierCurveTo(x1, yc + ROW_H * 0.2, x2, y1 - ROW_H * 0.35, x2, y1);
      }
      ctx.stroke();
    }
  }
  for (let i = start; i < end; i++) {
    const r = rows[i];
    const yc = (i - start) * ROW_H + ROW_H / 2;
    const x = laneX(clampLane(r.lane));
    const c = color(r.color);
    ctx.globalAlpha = opts.dimmed?.(i) ? 0.35 : 1;
    ctx.beginPath();
    if (i === opts.head) {
      ctx.arc(x, yc, 5, 0, Math.PI * 2);
      ctx.fillStyle = theme.bg;
      ctx.fill();
      ctx.lineWidth = 2.2;
      ctx.strokeStyle = theme.accent;
      ctx.stroke();
      ctx.lineWidth = 1.6;
    } else if (opts.merges(i)) {
      ctx.arc(x, yc, 3.2, 0, Math.PI * 2);
      ctx.fillStyle = theme.bg;
      ctx.fill();
      ctx.strokeStyle = c;
      ctx.stroke();
    } else {
      ctx.arc(x, yc, 3.6, 0, Math.PI * 2);
      ctx.fillStyle = c;
      ctx.fill();
    }
  }
  ctx.globalAlpha = 1;
}
