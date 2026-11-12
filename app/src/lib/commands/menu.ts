import type { Component } from 'svelte';

export interface MenuItem {
  label: string;
  icon?: Component<{ size?: number }>;
  agent?: string;
  hint?: string;
  checked?: boolean;
  disabled?: boolean;
  default?: boolean;
  danger?: boolean;
  separator?: boolean;
  more?: MenuItem[];
  run?: () => unknown;
}

export interface OpenMenu {
  items: MenuItem[];
  x: number;
  y: number;
  label: string;
}

export interface MenuRow {
  item: MenuItem;
  index: number;
  child: boolean;
}

export function menuRows(items: MenuItem[], expanded: ReadonlySet<number>): MenuRow[] {
  const rows: MenuRow[] = [];
  items.forEach((item, index) => {
    if (item.more && !item.more.length) return;
    rows.push({ item, index, child: false });
    if (item.more && expanded.has(index)) for (const c of item.more) rows.push({ item: c, index, child: true });
  });
  return rows;
}

export function pickItem(item: MenuItem, close: () => void, restore: () => void) {
  close();
  restore();
  item.run?.();
}
