import { api } from '../daemon/api';
import { DaemonError } from '../daemon/client';
import { sidebarItems, type SidebarItem } from '../session/grouping';
import type { HostLink } from './host-link.svelte';
import { applyEvent, type Snapshot } from '../reduce';
import type { DaemonEvent, Project } from '../types';
import { applyWsEvent, EMPTY_WS, itemWorkspaces, type WsData } from '../workspace/workspaces';

export function watch(link: HostLink, seed?: { data: Snapshot; ws: WsData; projects: Project[] }) {
  if (link.watching) return;
  if (seed) {
    link.data = { ...seed.data, activity: {} };
    link.ws = seed.ws;
    link.projects = seed.projects;
  }
  link.watching = true;
  link.reload = () => void load(link);
  link.off = link.client.subscribe((e) => onEvent(link, e));
  void load(link);
}

export function onEvent(link: HostLink, e: DaemonEvent) {
  if (/^(workspace|folder|group|project)\./.test(e.type)) {
    if (e.data?.project) {
      const p = e.data.project as Project;
      if (!link.projects.some((x) => x.path === p.path)) link.projects = [...link.projects, p];
    } else link.ws = applyWsEvent(link.ws, e);
    return;
  }
  const prev = link.data;
  const next = applyEvent(prev, e);
  if (next === prev) return;
  link.data = next;
  link.onChange?.(link, prev, next, e);
}

const loads = new WeakMap<HostLink, number>();

export async function load(link: HostLink) {
  if (!link.ready || !link.watching) return;
  const seq = (loads.get(link) ?? 0) + 1;
  loads.set(link, seq);
  const a = api(link.client);
  const ws = async (): Promise<WsData | null> => {
    try {
      const [workspaces, folders, groups] = await Promise.all([a.workspaces(), a.folders(), a.groups()]);
      return { workspaces, folders, groups };
    } catch (e) {
      return e instanceof DaemonError && e.status === 404 ? EMPTY_WS : null;
    }
  };
  const [lines, terms, tasks, projects, w] = await Promise.all([a.lines().catch(() => null), a.terms().catch(() => null), a.tasks().catch(() => null), a.projects().catch(() => null), ws()]);
  if (seq !== loads.get(link) || !link.watching) return;
  link.data = { lines: lines ?? link.data.lines, terms: terms ?? link.data.terms, tasks: tasks ?? link.data.tasks, activity: {} };
  if (projects) link.projects = projects;
  if (w) link.ws = w;
}

export function itemsOf(link: HostLink): { items: SidebarItem[]; itemWs: Map<string, string> } {
  if (!link.watching) return { items: [], itemWs: new Map() };
  const items = sidebarItems(link.data.lines, link.data.terms, link.data.tasks, {});
  return { items, itemWs: itemWorkspaces(items, link.data, link.ws.workspaces, link.projects) };
}
