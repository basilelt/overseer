import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

type Target = { kind: 'Any' | 'App' } | { kind: 'AnyLabel' | 'Window' | 'Webview' | 'WebviewWindow'; label: string };
type Listener = { event: string; target: Target; handler: number };

let callbacks: Map<number, (e: unknown) => void>;
let listeners: Listener[];

const labelOf = (t: Target) => ('label' in t ? t.label : null);

// Listener filters of tauri/src/manager/{webview,window,mod}.rs. With tauri's `unstable` feature every webview is a
// WindowChild, so real drops always take the `webview` route.
const ROUTES: Record<string, (t: Target) => boolean> = {
  webview: (t) => (t.kind === 'Webview' || t.kind === 'WebviewWindow') && labelOf(t) === 'main',
  window: (t) => (t.kind === 'Window' || t.kind === 'WebviewWindow') && labelOf(t) === 'main',
  anyLabel: (t) => t.kind !== 'Any' && t.kind !== 'App' && labelOf(t) === 'main',
};

function emit(route: keyof typeof ROUTES, event: string, payload: unknown) {
  for (const l of listeners) {
    if (l.event === event && (l.target.kind === 'Any' || ROUTES[route](l.target))) callbacks.get(l.handler)?.({ event, id: 1, payload });
  }
}

function zone() {
  const log: string[] = [];
  const el = {
    isConnected: true,
    getClientRects: () => [{}],
    closest: () => null,
    contains: () => false,
  } as unknown as Element;
  return { log, z: { el: () => el, rank: 1, hover: (on: boolean) => log.push(on ? 'on' : 'off'), drop: (p: string[]) => log.push(`drop:${p.join(',')}`) } };
}

describe('drop.ts against the real @tauri-apps/api event routing', () => {
  beforeEach(() => {
    vi.resetModules();
    callbacks = new Map();
    listeners = [];
    let next = 1;
    vi.stubGlobal('window', {
      devicePixelRatio: 2,
      __TAURI_INTERNALS__: {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
        transformCallback: (fn: (e: unknown) => void) => {
          const id = next++;
          callbacks.set(id, fn);
          return id;
        },
        invoke: async (cmd: string, args: Listener) => {
          if (cmd === 'plugin:event|listen') listeners.push({ event: args.event, target: args.target, handler: args.handler });
          return listeners.length;
        },
      },
    });
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh)' });
    vi.stubGlobal('document', { elementFromPoint: () => null });
  });
  afterEach(() => vi.unstubAllGlobals());

  for (const route of Object.keys(ROUTES)) {
    it(`delivers a drop emitted on the ${route} route exactly once`, async () => {
      const { dropZone } = await import('./drop');
      const a = zone();
      dropZone(a.z);
      await vi.waitFor(() => expect(listeners.filter((l) => l.event === 'tauri://drag-drop')).toHaveLength(1));
      emit(route, 'tauri://drag-over', { position: { x: 5, y: 5 } });
      emit(route, 'tauri://drag-drop', { paths: ['/tmp/a.png'], position: { x: 5, y: 5 } });
      expect(a.log).toEqual(['on', 'off', 'drop:/tmp/a.png']);
    });
  }
});
