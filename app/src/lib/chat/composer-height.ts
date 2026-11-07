const heights = new Map<symbol, number>();

function publish() {
  if (typeof document === 'undefined') return;
  const style = document.documentElement.style;
  const tallest = Math.max(0, ...heights.values());
  if (tallest > 0) style.setProperty('--composer-h', `${tallest}px`);
  else style.removeProperty('--composer-h');
}

export function reportComposerHeight(key: symbol, height: number) {
  if (height > 0) heights.set(key, height);
  else heights.delete(key);
  publish();
}

export function clearComposerHeight(key: symbol) {
  heights.delete(key);
  publish();
}
