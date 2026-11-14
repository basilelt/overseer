import { hostId } from '../hooks';
import { t } from '../i18n.svelte';
import { app } from '../state.svelte';
import type { LidPower } from '../types';

export const lid = $state<{ status: LidPower | null; host: string | null; busy: boolean }>({ status: null, host: null, busy: false });

export function lidAvailable(): boolean {
  return app.connected && app.conn.phase === 'connected' && app.conn.target.kind === 'local' && !app.mobile;
}

export async function loadLid(force = false) {
  if (!lidAvailable()) return;
  const host = hostId(app.conn);
  if (!force && lid.host === host && lid.status) return;
  lid.host = host;
  try {
    lid.status = await app.api.lidPower();
  } catch {
    lid.status = null;
  }
}

export async function setLid(el: HTMLInputElement) {
  const on = el.checked;
  if (lid.busy) {
    el.checked = !on;
    return;
  }
  lid.busy = true;
  try {
    lid.status = await app.api.setLidPower(on);
  } catch (e) {
    el.checked = !on;
    const error = String((e as Error)?.message ?? e);
    if (lid.status) lid.status = { ...lid.status, error };
    app.notify(t('settings.lidClosedFailed', { error }), 'error');
  } finally {
    lid.busy = false;
  }
}
