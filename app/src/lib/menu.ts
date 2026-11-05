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
  run?: () => unknown;
}

export interface OpenMenu {
  items: MenuItem[];
  x: number;
  y: number;
  label: string;
}
