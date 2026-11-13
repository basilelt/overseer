import type { Api } from '../daemon/api';
import { saveSettings, settings } from './settings.svelte';

type Concurrency = Pick<Api, 'autoConcurrency' | 'setAutoConcurrency'>;

export async function pullAutoConcurrency(api: Concurrency): Promise<void> {
  if (!settings.autoConcurrencySynced && settings.autoConcurrency !== 2) {
    const r = await api.setAutoConcurrency(settings.autoConcurrency);
    saveSettings({ autoConcurrency: r.value, autoConcurrencySynced: true });
    return;
  }
  const r = await api.autoConcurrency();
  saveSettings({ autoConcurrency: r.value, autoConcurrencySynced: true });
}

export async function pushAutoConcurrency(api: Concurrency, value: number): Promise<number> {
  const r = await api.setAutoConcurrency(value);
  saveSettings({ autoConcurrency: r.value, autoConcurrencySynced: true });
  return r.value;
}
