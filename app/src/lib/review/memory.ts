import type { Indicator } from './map.svelte';

export interface ReviewMemory {
  view: 'list' | 'map';
  selected: string | null;
  hot: string | null;
  ind: Indicator;
  all: boolean;
  sortRisk: boolean;
  scrollTop: number;
  folded: Record<string, boolean>;
  collapsed: Record<string, boolean>;
}

export const MEMORY_MAX = 30;

const kept = new Map<string, ReviewMemory>();

export function remember(key: string, m: ReviewMemory): void {
  kept.delete(key);
  kept.set(key, m);
  if (kept.size > MEMORY_MAX) kept.delete(kept.keys().next().value!);
}

export function recall(key: string): ReviewMemory | undefined {
  return kept.get(key);
}

export function forgetAll(): void {
  kept.clear();
}
