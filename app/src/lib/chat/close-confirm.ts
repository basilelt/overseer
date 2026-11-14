import { OneShot } from '../timer';

export function closeConfirm(set: (armed: boolean) => void, ms = 3000) {
  const timer = new OneShot();
  return {
    arm() {
      set(true);
      timer.set(() => set(false), ms);
    },
    reset() {
      timer.clear();
      set(false);
    },
  };
}
