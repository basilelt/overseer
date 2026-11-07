import { onPresence, presence } from './presence';

// Calls tick(Date.now()) now and every period, only while the window is visible; returns the cleanup.
export function visibleClock(tick: (now: number) => void, period = 30_000): () => void {
  let id: ReturnType<typeof setInterval> | undefined;
  const sync = () => {
    clearInterval(id);
    id = undefined;
    if (!presence().visible) return;
    tick(Date.now());
    id = setInterval(() => tick(Date.now()), period);
  };
  sync();
  const off = onPresence(sync);
  return () => {
    off();
    clearInterval(id);
  };
}
