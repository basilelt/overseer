export function closeConfirm(set: (armed: boolean) => void, ms = 3000) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const clear = () => {
    clearTimeout(timer);
    timer = undefined;
  };
  return {
    arm() {
      clear();
      set(true);
      timer = setTimeout(() => {
        timer = undefined;
        set(false);
      }, ms);
    },
    reset() {
      clear();
      set(false);
    },
  };
}
