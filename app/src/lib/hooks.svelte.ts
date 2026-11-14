import { agentName } from './agents';
import { hostId, outcome } from './hooks';
import { t } from './i18n.svelte';
import { saveSettings, settings } from './settings/settings.svelte';
import { app } from './state.svelte';
import type { HooksStatus } from './types';

export const hooks = $state<{ status: HooksStatus | null; host: string | null; later: string[]; busy: boolean }>({ status: null, host: null, later: [], busy: false });

export async function loadHooks(force = false) {
  if (!app.connected || app.conn.phase !== 'connected') return;
  const host = hostId(app.conn);
  if (!force && hooks.host === host && hooks.status) return;
  hooks.host = host;
  try {
    hooks.status = await app.api.hooksStatus();
  } catch {
    hooks.status = null;
  }
}

const names = (id: string) => agentName(app.agents, id);

export async function setHooks(agents: string[], on: boolean) {
  if (!agents.length || hooks.busy) return;
  hooks.busy = true;
  try {
    const run = on ? await app.api.installHooks(agents) : await app.api.uninstallHooks(agents);
    const o = outcome(run, names);
    if (o.failed.length) app.notify(t('hooks.failed', { error: o.failed.map((f) => `${f.agent}: ${f.error}`).join('; ') }), 'error');
    else if (o.codexTrust) app.notify(t('hooks.codexTrust'));
    else if (o.done.length) app.notify(t(on ? 'hooks.installed' : 'hooks.removed', { agents: o.done.join(', ') }));
  } catch (e) {
    app.notify(t('hooks.failed', { error: String((e as Error)?.message ?? e) }), 'error');
  } finally {
    hooks.busy = false;
    await loadHooks(true);
  }
}

export function hooksLater() {
  if (hooks.host) hooks.later = [...hooks.later, hooks.host];
}

export function hooksNever() {
  if (hooks.host && !settings.hooksNever.includes(hooks.host)) saveSettings({ hooksNever: [...settings.hooksNever, hooks.host] });
}
