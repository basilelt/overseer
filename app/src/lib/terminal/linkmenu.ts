import AppWindow from '@lucide/svelte/icons/app-window';
import Copy from '@lucide/svelte/icons/copy';
import ExternalLink from '@lucide/svelte/icons/external-link';
import Globe from '@lucide/svelte/icons/globe';
import { t } from '../i18n.svelte';
import type { InstalledBrowser, LinkRoute } from '../links';
import type { MenuItem } from '../commands/menu';

export function linkItems(list: InstalledBrowser[], act: { inside: () => void; outside: (r: LinkRoute) => void; copy: () => void }): MenuItem[] {
  return [
    { label: t('link.inOverseer'), icon: AppWindow, run: act.inside },
    { label: t('link.inDefault'), icon: ExternalLink, run: () => act.outside({ kind: 'default' }) },
    ...list.map((b): MenuItem => ({ label: t('link.in', { name: b.name }), icon: Globe, run: () => act.outside({ kind: 'browser', id: b.id }) })),
    { label: '', separator: true },
    { label: t('link.copy'), icon: Copy, run: act.copy },
  ];
}
