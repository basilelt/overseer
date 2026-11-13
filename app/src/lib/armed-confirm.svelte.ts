import { CONFIRM_WINDOW_MS } from './timing';

export function armedConfirm(ms = CONFIRM_WINDOW_MS) {
  let armed = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const reset = () => {
    clearTimeout(timer);
    armed = false;
  };
  return {
    get armed() {
      return armed;
    },
    press(): boolean {
      if (armed) {
        reset();
        return true;
      }
      armed = true;
      timer = setTimeout(() => (armed = false), ms);
      return false;
    },
    reset,
  };
}
