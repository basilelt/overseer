import { lifecycle, patchLifecycle } from './lifecycle.svelte';

export async function toggleAutostart(input: { checked: boolean }) {
  await patchLifecycle({ autostart: input.checked });
  input.checked = lifecycle.status?.autostart.enabled ?? false;
}
