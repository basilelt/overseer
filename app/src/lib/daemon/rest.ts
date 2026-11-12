import { setIdle } from '../presence';

export const IDLE_MS = 60_000;
const INPUT = ['keydown', 'pointerdown', 'pointermove', 'wheel', 'focus'];

export function startRest(): () => void {
  let last = Date.now();
  let resting = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const check = () => {
    const left = last + IDLE_MS - Date.now();
    timer = left > 0 ? setTimeout(check, left) : undefined;
    if (!timer) setIdle((resting = true));
  };
  const input = () => {
    last = Date.now();
    timer ??= setTimeout(check, IDLE_MS);
    if (resting) setIdle((resting = false));
  };
  input();
  for (const t of INPUT) window.addEventListener(t, input, true);
  return () => {
    clearTimeout(timer);
    for (const t of INPUT) window.removeEventListener(t, input, true);
    if (resting) setIdle(false);
  };
}
