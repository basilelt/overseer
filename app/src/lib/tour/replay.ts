import { tick } from 'svelte';
import { app } from '../state.svelte';
import { ui, isPhone } from '../mobile/ui.svelte';
import { feedback } from '../feedback.svelte';
import { tour } from './tour.svelte';

export async function replayTour() {
  app.open(null);
  app.menu = null;
  feedback.close();
  ui.drawer = false;
  ui.panel = false;
  if (isPhone()) ui.tab = 'agents';
  if (isPhone() || app.view === 'browser') app.view = 'agents';
  await tick();
  tour.start('full');
}
