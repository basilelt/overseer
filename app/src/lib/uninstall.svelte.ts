import { isTauri } from './daemon/client';
import { t } from './i18n.svelte';
import { app } from './state.svelte';
import type { UninstallReport } from './types';

export type UninstallPhase = 'idle' | 'planning' | 'confirm' | 'applying' | 'done' | 'error';

export const uninstall = $state<{ phase: UninstallPhase; report: UninstallReport | null; error: string }>({ phase: 'idle', report: null, error: '' });

function fail(e: unknown) {
  uninstall.error = String((e as Error)?.message ?? e) || t('uninstall.failed');
  uninstall.phase = 'error';
}

export function resetUninstall() {
  if (uninstall.phase === 'planning' || uninstall.phase === 'applying') return;
  uninstall.phase = 'idle';
  uninstall.report = null;
  uninstall.error = '';
}

export async function planUninstall() {
  if (uninstall.phase !== 'idle') return;
  uninstall.phase = 'planning';
  try {
    uninstall.report = await app.api.uninstall(true);
    uninstall.phase = 'confirm';
  } catch (e) {
    fail(e);
  }
}

export async function applyUninstall() {
  if (uninstall.phase !== 'confirm') return;
  uninstall.phase = 'applying';
  try {
    uninstall.report = await app.api.uninstall(false);
    uninstall.phase = 'done';
    await app.disconnect();
  } catch (e) {
    fail(e);
  }
}

export function canQuit(): boolean {
  return isTauri() && app.conn.target.kind === 'local';
}

export async function quitApp() {
  try {
    const { exit } = await import('@tauri-apps/plugin-process');
    await exit(0);
  } catch (e) {
    fail(e);
  }
}
