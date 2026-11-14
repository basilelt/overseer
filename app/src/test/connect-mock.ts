import { LOCAL_TARGET } from '../lib/conn';
import { setClient } from '../lib/daemon/client';
import type { MockDaemon } from '../lib/daemon/mock';
import { app } from '../lib/state.svelte';
import type { HostTarget } from '../lib/types';

export async function connectMock<T extends MockDaemon>(daemon: T, target: HostTarget = LOCAL_TARGET): Promise<T> {
  setClient(daemon);
  app.client = null;
  await app.connect(target);
  return daemon;
}
