import { app } from './state.svelte';
import { hostId } from './hooks';
import { applySettings, dismissForever, isDismissed, shouldOffer } from './switchboard';
import type { SwitchboardResult, SwitchboardStatus } from './types';

export const switchboard = $state<{ status: SwitchboardStatus | null; host: string | null; offered: string[]; checked: boolean; busy: boolean; error: string; done: SwitchboardResult | null }>({
  status: null,
  host: null,
  offered: [],
  checked: false,
  busy: false,
  error: '',
  done: null,
});

export async function loadSwitchboard(force = false) {
  if (!app.connected || app.conn.phase !== 'connected') return;
  const host = hostId(app.conn);
  if (!force && switchboard.host === host && switchboard.status) return;
  switchboard.host = host;
  try {
    switchboard.status = await app.api.switchboardStatus();
  } catch {
    switchboard.status = null;
  }
}

export async function offerSwitchboard() {
  await loadSwitchboard();
  if (!app.overlay && shouldOffer(switchboard.status, app.conn, isDismissed(), switchboard.offered)) {
    switchboard.offered = [...switchboard.offered, hostId(app.conn)];
    app.open('switchboardImport');
  }
  switchboard.checked = true;
}

export function resetImport() {
  switchboard.error = '';
  switchboard.done = null;
}

export function switchboardNever() {
  dismissForever();
}

export function reloadApp() {
  location.reload();
}

export async function runImport(): Promise<boolean> {
  if (switchboard.busy) return false;
  switchboard.busy = true;
  switchboard.error = '';
  try {
    const result = await app.api.importSwitchboard();
    const fresh = await app.api.switchboardStatus().catch(() => switchboard.status);
    applySettings(fresh?.settings ?? switchboard.status?.settings);
    switchboard.done = result;
    return true;
  } catch (e) {
    switchboard.error = String((e as Error)?.message ?? e);
    return false;
  } finally {
    switchboard.busy = false;
  }
}
