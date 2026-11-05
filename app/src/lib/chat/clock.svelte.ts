import { active, onPresence } from '../presence';

export const clock = $state({ now: Date.now() });

let users = 0;
let timer: ReturnType<typeof setInterval> | undefined;
let off: (() => void) | undefined;

function sync() {
  const want = users > 0 && active();
  if (want && !timer) {
    clock.now = Date.now();
    timer = setInterval(() => (clock.now = Date.now()), 1000);
  } else if (!want && timer) {
    clearInterval(timer);
    timer = undefined;
  }
  if (users > 0 && !off) off = onPresence(sync);
  else if (!users && off) {
    off();
    off = undefined;
  }
}

export function tickWhile(): () => void {
  users++;
  sync();
  let done = false;
  return () => {
    if (done) return;
    done = true;
    users--;
    sync();
  };
}
