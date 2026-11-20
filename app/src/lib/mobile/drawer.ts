import { app } from '../state.svelte';
import { ui } from './ui.svelte';

let mark = 0;
let current: Event | null = null;

export const drawerPick = {
  begin(e?: Event) {
    current = e ?? null;
    mark = app.picks;
  },
  end(e?: Event) {
    if (e && e !== current) return;
    current = null;
    if (app.picks !== mark) ui.drawer = false;
  },
  watch(node: HTMLElement) {
    node.addEventListener('click', drawerPick.begin, true);
    window.addEventListener('click', drawerPick.end);
    return {
      destroy() {
        node.removeEventListener('click', drawerPick.begin, true);
        window.removeEventListener('click', drawerPick.end);
      },
    };
  },
};
