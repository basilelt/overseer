import type { DaemonClient } from './client';
import type { SourceItem, SourceItems, TaskSource, TaskSourceKind } from './types';

const enc = encodeURIComponent;

export function sourcesApi(c: DaemonClient) {
  return {
    list: (project: string) => c.request<TaskSource[]>('GET', `/v1/projects/sources?project=${enc(project)}`),
    add: (project: string, kind: TaskSourceKind, target: string) => c.request<TaskSource>('POST', '/v1/projects/sources', { project, kind, target }),
    remove: (id: string) => c.request<{ ok: boolean }>('DELETE', `/v1/projects/sources/${enc(id)}`),
    items: (project: string) => c.request<SourceItems>('GET', `/v1/projects/items?project=${enc(project)}`),
  };
}

export function filterItems(items: SourceItem[], query: string): SourceItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter((i) => i.key.toLowerCase().includes(q) || i.title.toLowerCase().includes(q));
}

export function fillFrom(item: SourceItem): { title: string; description: string; ticketKey: string } {
  const ref = item.url || item.key;
  return {
    title: item.title,
    description: item.body.trim() ? `${item.body.trim()}\n\n${ref}` : ref,
    ticketKey: item.ticketKey ?? '',
  };
}
