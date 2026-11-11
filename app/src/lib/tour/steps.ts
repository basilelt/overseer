export type StepId = 'host' | 'workspaces' | 'tabs' | 'view' | 'panel' | 'kanban' | 'review' | 'chips' | 'notify' | 'profiles' | 'palette' | 'paletteKey' | 'organize' | 'settings' | 'feedback' | 'demo' | 'gpuTerminal' | 'worktreeCleanup' | 'switchboard' | 'clickCursor' | 'palettes' | 'skills' | 'autoApprove' | 'merged' | 'uninstall' | 'autoUpdate' | 'subjects' | 'sourceControl' | 'reviewMap';

export interface Step {
  id: StepId;
  targets: string[];
  since: string;
  center?: boolean;
}

export const STEPS: Step[] = [
  { id: 'host', targets: ['host'], since: '0.15' },
  { id: 'demo', targets: ['demo', 'host'], since: '0.27' },
  { id: 'workspaces', targets: ['workspaces'], since: '0.15' },
  { id: 'tabs', targets: ['tabs'], since: '0.15' },
  { id: 'view', targets: ['view-toggle'], since: '0.15' },
  { id: 'panel', targets: ['panel'], since: '0.15' },
  { id: 'kanban', targets: ['kanban'], since: '0.15' },
  { id: 'review', targets: ['review'], since: '0.15' },
  { id: 'chips', targets: ['chips'], since: '0.15' },
  { id: 'notify', targets: ['hooks', 'settings'], since: '0.15' },
  { id: 'profiles', targets: ['settings'], since: '0.15' },
  { id: 'palette', targets: ['palette'], since: '0.15', center: true },
  { id: 'organize', targets: ['organize', 'workspaces'], since: '0.17' },
  { id: 'settings', targets: ['settings', 'palette'], since: '0.25' },
  { id: 'feedback', targets: ['feedback'], since: '0.15' },
  { id: 'paletteKey', targets: ['palette'], since: '0.25', center: true },
  { id: 'gpuTerminal', targets: ['settings', 'palette'], since: '0.28' },
  { id: 'worktreeCleanup', targets: ['palette'], since: '0.29', center: true },
  { id: 'switchboard', targets: ['switchboard', 'settings', 'palette'], since: '0.29' },
  { id: 'clickCursor', targets: ['tabs'], since: '0.30' },
  { id: 'palettes', targets: ['settings', 'palette'], since: '0.31' },
  { id: 'skills', targets: ['settings'], since: '0.33' },
  { id: 'autoApprove', targets: ['auto-approve', 'settings', 'palette'], since: '0.34' },
  { id: 'merged', targets: ['merged', 'kanban'], since: '0.34' },
  { id: 'uninstall', targets: ['uninstall', 'settings'], since: '0.34' },
  { id: 'autoUpdate', targets: ['updates', 'settings'], since: '0.35' },
  { id: 'subjects', targets: ['subjects', 'workspaces'], since: '0.36' },
  { id: 'sourceControl', targets: ['git-button', 'palette'], since: '0.36' },
  { id: 'reviewMap', targets: ['review-map', 'review'], since: '0.36' },
];

export type Mode = 'full' | 'whatsNew';

const LEGACY: Record<number, string> = { 1: '0.15', 2: '0.17' };

export function compareVersions(a: string, b: string): number {
  const x = a.split('.').map(Number);
  const y = b.split('.').map(Number);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

export const TOUR_VERSION = STEPS.reduce((v, s) => (compareVersions(s.since, v) > 0 ? s.since : v), '0');

export function seenVersion(seen: number | string | undefined): string | undefined {
  if (typeof seen === 'string') return seen;
  return seen ? (LEGACY[seen] ?? LEGACY[2]) : undefined;
}

export function modeFor(seen: number | string | undefined, version = TOUR_VERSION): Mode | null {
  const s = seenVersion(seen);
  if (!s) return 'full';
  return compareVersions(s, version) >= 0 ? null : 'whatsNew';
}

export function stepsFor(mode: Mode, seen: number | string | undefined, steps: Step[] = STEPS): Step[] {
  if (mode === 'full') return steps;
  const s = seenVersion(seen) ?? '0';
  return steps.filter((x) => compareVersions(x.since, s) > 0).sort((a, b) => compareVersions(a.since, b.since));
}

export function targetOf(step: Step, has: (name: string) => boolean): string | null {
  return step.targets.find(has) ?? null;
}

export function visibleSteps(steps: Step[], has: (name: string) => boolean): Step[] {
  return steps.filter((s) => s.center || targetOf(s, has) !== null);
}

export type KeyAction = 'next' | 'back' | 'close';

export function keyAction(key: string): KeyAction | null {
  if (key === 'ArrowRight') return 'next';
  if (key === 'ArrowLeft') return 'back';
  if (key === 'Escape') return 'close';
  return null;
}

export interface Rect {
  top: number;
  left: number;
  width: number;
  height: number;
}

export function union(rects: Rect[]): Rect | null {
  const r = rects.filter((x) => x.width > 0 && x.height > 0);
  if (!r.length) return null;
  const top = Math.min(...r.map((x) => x.top));
  const left = Math.min(...r.map((x) => x.left));
  const bottom = Math.max(...r.map((x) => x.top + x.height));
  const right = Math.max(...r.map((x) => x.left + x.width));
  return { top, left, width: right - left, height: bottom - top };
}

export interface Placement {
  top: number;
  left: number;
}

export function placeCard(target: Rect, card: { width: number; height: number }, view: { width: number; height: number }, gap = 12, margin = 12): Placement {
  const clampX = (x: number) => Math.max(margin, Math.min(view.width - card.width - margin, x));
  const clampY = (y: number) => Math.max(margin, Math.min(view.height - card.height - margin, y));
  const below = target.top + target.height + gap;
  if (below + card.height + margin <= view.height) return { top: below, left: clampX(target.left) };
  const above = target.top - gap - card.height;
  if (above >= margin) return { top: above, left: clampX(target.left) };
  const right = target.left + target.width + gap;
  if (right + card.width + margin <= view.width) return { top: clampY(target.top), left: right };
  const left = target.left - gap - card.width;
  if (left >= margin) return { top: clampY(target.top), left };
  return { top: clampY(view.height - card.height - margin), left: clampX((view.width - card.width) / 2) };
}
