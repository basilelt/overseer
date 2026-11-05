import type { PortRow } from './ports';
import type { app as App } from './state.svelte';

export async function stopProcess(app: Pick<typeof App, 'client' | 'notify'>, row: PortRow) {
  if (!row.pid || !app.client) return;
  try {
    await app.client.request('POST', `/v1/ports/${row.port}/kill`, { pid: row.pid, confirm: true });
  } catch (e) {
    app.notify(String((e as Error)?.message ?? e), 'error');
  }
}
