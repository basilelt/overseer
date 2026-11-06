import { onPresence, presence } from './presence';

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
