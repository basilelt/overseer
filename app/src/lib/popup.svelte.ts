import { untrack } from 'svelte';
import { diag } from './diag/diag.svelte';
import { feedback } from './feedback/feedback.svelte';
import { ui } from './mobile/ui.svelte';
import { rec } from './record/record-state.svelte';
import { app } from './state.svelte';
import { tour } from './tour/tour.svelte';
import { organizer } from './workspace/organize.svelte';

let held = $state(0);

export function holdPopup(): () => void {
  untrack(() => held++);
  return () => untrack(() => held--);
}

class Popup {
  /**
   * True while any popup (modal, sheet, menu, drawer, tour) is open.
   * Native webviews (the browser tab, session pages) composite above the DOM,
   * so they must hide while this is true, or they would paint over the popup.
   */
  get open(): boolean {
    return (
      held > 0 ||
      !!app.overlay ||
      !!app.menu ||
      feedback.open ||
      !!diag.mode ||
      rec.phase === 'setup' ||
      !!organizer.phase ||
      ui.drawer ||
      !!tour.phase
    );
  }
}

export const popup = new Popup();
