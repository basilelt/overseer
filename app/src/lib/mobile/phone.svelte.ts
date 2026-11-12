import { app } from '../state.svelte';
import { isPhone } from './ui.svelte';

export function phoneOrMobile(): boolean {
  return app.mobile || isPhone();
}
