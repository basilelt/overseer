import type { Workspace } from '../types';

export type SubjectBody = { kind: 'subject'; name: string; parentId: string } | { path: string; name?: string; parentId: string; cwd?: string };

const trim = (p: string) => p.trim().replace(/\/+$/, '');

// A folder picked in the browser, shown relative to the parent; a folder outside it stays absolute so the daemon can refuse it.
export function relativeTo(parent: Pick<Workspace, 'path'>, abs: string): string {
  const root = trim(parent.path ?? '');
  const p = trim(abs);
  if (p === root) return '';
  return root && p.startsWith(`${root}/`) ? p.slice(root.length + 1) : p;
}

// The POST /v1/workspaces body of the "New subject" dialog: a path-less subject, or a nested folder or submodule.
export function subjectBody(parent: Pick<Workspace, 'id' | 'path'>, rawPath: string, rawName: string, startAtRoot: boolean): SubjectBody | null {
  const rel = trim(rawPath).replace(/^\.\/+/, '');
  const name = rawName.trim();
  if (!rel) return name ? { kind: 'subject', name, parentId: parent.id } : null;
  const root = trim(parent.path ?? '');
  const path = rel.startsWith('/') || rel.startsWith('~') ? rel : `${root}/${rel}`;
  return { path, ...(name ? { name } : {}), parentId: parent.id, ...(startAtRoot && root ? { cwd: root } : {}) };
}
