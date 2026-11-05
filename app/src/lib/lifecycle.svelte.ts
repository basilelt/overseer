import { hostId } from './hooks';
import { t } from './i18n.svelte';
import { lingerOff } from './lifecycle';
import { saveSettings, settings } from './settings.svelte';
import { app } from './state.svelte';
import type { Lifecycle, LifecyclePatch } from './types';

export const lifecycle = $state<{ status: Lifecycle | null; host: string | null; busy: boolean }>({ status: null, host: null, busy: false });

export async function loadLifecycle(force = false) {
  if (!app.connected || app.conn.phase !== 'connected') return;
  const host = hostId(app.conn);
  if (!force && lifecycle.host === host && lifecycle.status) return;
  lifecycle.host = host;
  try {
    lifecycle.status = await app.api.lifecycle();
  } catch {
    lifecycle.status = null;
  }
}

export async function patchLifecycle(patch: LifecyclePatch) {
  if (lifecycle.busy) return;
  lifecycle.busy = true;
  try {
    const next = await app.api.patchLifecycle(patch);
    lifecycle.status = next;
    if (next.error) app.notify(t('lifecycle.failed', { error: next.error }), 'error');
    else if (patch.autostart && lingerOff(next)) app.notify(t('lifecycle.lingerOff', { command: next.autostart.lingerCommand ?? '' }));
    else if (patch.autostart) app.notify(t('lifecycle.enabled'));
    else if (patch.autostart === false) app.notify(t('lifecycle.disabled'));
    else if (patch.linger) app.notify(t('lifecycle.lingerOn'));
  } catch (e) {
    app.notify(t('lifecycle.failed', { error: String((e as Error)?.message ?? e) }), 'error');
  } finally {
    lifecycle.busy = false;
  }
}

export function keepRunningAnswered() {
  const id = hostId(app.conn);
  const offered = settings.keepRunningOffered ?? [];
  if (!offered.includes(id)) saveSettings({ keepRunningOffered: [...offered, id] });
}
