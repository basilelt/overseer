import { invoke } from '@tauri-apps/api/core';
import { isTauri } from './client';
import type { InstalledBrowser, LinkRoute } from './links';

class Browsers {
  list = $state.raw<InstalledBrowser[] | null>(null);
  private pending: Promise<InstalledBrowser[]> | null = null;

  load(desktop: boolean): Promise<InstalledBrowser[]> {
    if (!desktop || !isTauri()) return Promise.resolve([]);
    if (this.list) return Promise.resolve(this.list);
    return (this.pending ??= invoke<InstalledBrowser[]>('browsers_list')
      .catch(() => [])
      .then((l) => (this.list = l)));
  }
}

export const browsers = new Browsers();

export async function openOutside(url: string, route: LinkRoute): Promise<void> {
  if (!isTauri()) return void window.open(url, '_blank', 'noopener');
  if (route.kind === 'browser') {
    const opened = await invoke('browser_open_with', { id: route.id, url }).then(
      () => true,
      () => false,
    );
    if (opened) return;
  }
  await invoke('open_external', { url });
}
