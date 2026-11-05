export interface DirEntry {
  name: string;
  path: string;
  git: boolean;
}

export interface DirListing {
  path: string;
  parent: string | null;
  home: string;
  dirs: DirEntry[];
  truncated?: boolean;
}

export interface Crumb {
  label: string;
  path: string;
}

const trim = (p: string) => (p.length > 1 ? p.replace(/\/+$/, '') : p);

export function baseName(path: string): string {
  return trim(path.trim()).split('/').pop() ?? '';
}

export function shortPath(p: string): string {
  return p.replace(/^\/(home|Users)\/[^/]+(?=\/|$)/, '~');
}

export function tildePath(path: string, home: string): string {
  const p = trim(path);
  const h = trim(home);
  if (!h || h === '/') return p;
  if (p === h) return '~';
  return p.startsWith(`${h}/`) ? `~${p.slice(h.length)}` : p;
}

export function crumbs(path: string, home: string): Crumb[] {
  const p = trim(path);
  const h = trim(home);
  const inHome = !!h && h !== '/' && (p === h || p.startsWith(`${h}/`));
  const out: Crumb[] = [inHome ? { label: '~', path: h } : { label: '/', path: '/' }];
  let at = inHome ? h : '';
  for (const seg of p.slice(inHome ? h.length : 0).split('/')) {
    if (!seg) continue;
    at = `${at}/${seg}`;
    out.push({ label: seg, path: at });
  }
  return out;
}
