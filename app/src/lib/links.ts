export type LinkTarget = 'overseer' | 'default' | 'browser';

export interface InstalledBrowser {
  id: string;
  name: string;
}

export interface LinkPrefs {
  linkTarget: LinkTarget;
  linkBrowser: string;
  linkAsk: boolean;
}

export interface LinkFrom {
  key?: string | null;
  termId?: string;
}

export interface LinkGesture {
  shift?: boolean;
  mod?: boolean;
}

export type LinkRoute = { kind: 'ask' } | { kind: 'overseer' } | { kind: 'default' } | { kind: 'browser'; id: string };

export function resolveLink(prefs: LinkPrefs, g: LinkGesture, ctx: { phone: boolean; inTerminal?: boolean; browsers?: InstalledBrowser[] | null }): LinkRoute {
  if (g.shift) return { kind: 'ask' };
  if (g.mod && !ctx.inTerminal) return { kind: 'default' };
  if (prefs.linkAsk) return { kind: 'ask' };
  if (prefs.linkTarget === 'default') return { kind: 'default' };
  if (prefs.linkTarget === 'browser') {
    if (ctx.phone || !prefs.linkBrowser) return { kind: 'default' };
    if (ctx.browsers && !ctx.browsers.some((b) => b.id === prefs.linkBrowser)) return { kind: 'default' };
    return { kind: 'browser', id: prefs.linkBrowser };
  }
  return { kind: 'overseer' };
}

export function isWebLink(url: string): boolean {
  return /^https?:\/\//i.test(url);
}

export function gesture(e: MouseEvent): LinkGesture & { at: MouseEvent } {
  return { shift: e.shiftKey, mod: e.metaKey || e.ctrlKey, at: e };
}
