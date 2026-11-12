import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right';
import AppWindow from '@lucide/svelte/icons/app-window';
import ChevronsUpDown from '@lucide/svelte/icons/chevrons-up-down';
import RotateCw from '@lucide/svelte/icons/rotate-cw';
import Unplug from '@lucide/svelte/icons/unplug';
import { t } from '../i18n.svelte';
import { toggled } from './host-choice';
import { hostName } from './hosts';
import type { MenuItem } from '../commands/menu';
import { saveSettings, settings } from '../settings/settings.svelte';
import { app } from '../state.svelte';

export { hostName };

export function toggleFold(key: string) {
  saveSettings({ hostsFolded: toggled(settings.hostsFolded, key) });
}

export async function openOnHost(key: string, run: () => void) {
  if (app.link?.key !== key) await app.switchHost(key);
  run();
}

export function hostMenu(key: string): MenuItem[] {
  const link = app.hosts.find((l) => l.key === key);
  if (!link) return [];
  const name = hostName(link.target);
  const active = app.link === link;
  const folded = settings.hostsFolded?.includes(key) ?? false;
  const failed = (active ? app.conn : link.conn).phase === 'error';
  return [
    ...(active ? [] : [{ label: t('hosts.makeActive'), icon: ArrowLeftRight, run: () => app.switchHost(key) }]),
    ...(failed ? [{ label: t('common.retry'), icon: RotateCw, run: () => (active ? app.connect(link.target) : app.removeHost(key).then(() => app.addHost(link.target, false))) }] : []),
    { label: t(folded ? 'ws.expand' : 'ws.collapse', { name }), icon: ChevronsUpDown, run: () => toggleFold(key) },
    { label: t('hosts.moveToWindow'), icon: AppWindow, disabled: app.hosts.length < 2, run: () => app.moveToWindow(key) },
    { label: t('hosts.disconnect'), icon: Unplug, danger: true, run: () => app.removeHost(key) },
  ];
}
