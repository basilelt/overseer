import { app } from '../state.svelte';
import { isPhoneOr } from './ui.svelte';

export function phoneOrMobile(): boolean {
  return isPhoneOr(app.mobile);
}
