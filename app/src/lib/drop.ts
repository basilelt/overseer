import { isTauri } from './daemon/client';
import { isWindowsUA } from './platform';

export interface DropZone {
  el: () => Element | null | undefined;
  rank: number;
  hover: (on: boolean) => void;
  drop: (paths: string[]) => void;
}

export function dropPoint(pos: { x: number; y: number }, windows: boolean, dpr: number): { x: number; y: number } {
  const r = windows ? dpr || 1 : 1;
  return { x: pos.x / r, y: pos.y / r };
}

const shown = (el: Element) => el.isConnected && el.getClientRects().length > 0;
const inFocus = (el: Element) => !el.closest('.pane') || !!el.closest('.pane.focused');

export function pickZone(zones: DropZone[], hits: (Element | null)[]): DropZone | null {
  const live = zones.filter((z) => {
    const el = z.el();
    return !!el && shown(el);
  });
  for (const hit of hits) {
    if (!hit) continue;
    const under = live.filter((z) => z.el()!.contains(hit)).sort((a, b) => b.rank - a.rank)[0];
    if (under) return under;
  }
  return live.filter((z) => inFocus(z.el()!)).sort((a, b) => b.rank - a.rank)[0] ?? null;
}

const zones = new Set<DropZone>();
let hovered: DropZone | null = null;
let listening = false;

function at(pos: { x: number; y: number }): DropZone | null {
  const scaled = dropPoint(pos, isWindowsUA(), window.devicePixelRatio);
  const raw = dropPoint(pos, false, 1);
  return pickZone([...zones], [document.elementFromPoint(scaled.x, scaled.y), document.elementFromPoint(raw.x, raw.y)]);
}

function setHover(z: DropZone | null) {
  if (z === hovered) return;
  hovered?.hover(false);
  hovered = z;
  z?.hover(true);
}

function listen() {
  if (listening || !isTauri()) return;
  listening = true;
  void import('@tauri-apps/api/webviewWindow')
    .then(({ getCurrentWebviewWindow }) =>
      getCurrentWebviewWindow().onDragDropEvent(({ payload }) => {
        if (payload.type === 'leave') return setHover(null);
        const z = at(payload.position);
        if (payload.type !== 'drop') return setHover(z);
        setHover(null);
        z?.drop(payload.paths);
      }),
    )
    .catch(() => (listening = false));
}

export function dropZone(z: DropZone): () => void {
  zones.add(z);
  listen();
  return () => {
    zones.delete(z);
    if (hovered === z) setHover(null);
  };
}
