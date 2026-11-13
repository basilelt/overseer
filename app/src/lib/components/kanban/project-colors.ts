export const PROJECT_SWATCHES = ['#64748b', '#3b82f6', '#16a34a', '#d97706', '#9333ea', '#db2777', '#0891b2', '#dc2626'];

const FALLBACK_COLORS = ['#64748b', '#0891b2', '#7c3aed', '#db2777', '#65a30d', '#ea580c'];

export function fallbackProjectColor(name: string): string {
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return FALLBACK_COLORS[h % FALLBACK_COLORS.length];
}
