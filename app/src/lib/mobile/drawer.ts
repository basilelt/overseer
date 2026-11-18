import { app } from '../state.svelte';
import { ui } from './ui.svelte';

let mark = 0;

export const drawerPick = {
  begin() {
    mark = app.picks;
  },
  end() {
    if (app.picks !== mark) ui.drawer = false;
  },
  watch(node: HTMLElement) {
    node.addEventListener('click', drawerPick.begin, true);
    node.addEventListener('click', drawerPick.end);
    return {
      destroy() {
        node.removeEventListener('click', drawerPick.begin, true);
        node.removeEventListener('click', drawerPick.end);
      },
    };
  },
};
