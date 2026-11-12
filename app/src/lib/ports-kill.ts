import { portsApi } from './ports-api';
import type { PortRow } from './ports';
import type { app as App } from './state.svelte';

export async function stopProcess(app: Pick<typeof App, 'client' | 'notify'>, row: PortRow) {
  if (!row.pid || !app.client) return;
  try {
    await portsApi(app.client).killPort(row.port, row.pid);
  } catch (e) {
    app.notify(String((e as Error)?.message ?? e), 'error');
  }
}
