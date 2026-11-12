export function platformName(): string {
  return typeof navigator !== 'undefined' ? navigator.platform : '';
}

export function userAgent(): string {
  return typeof navigator !== 'undefined' ? navigator.userAgent : '';
}

export const isMac = /mac/i.test(platformName());

export function isWindowsUA(): boolean {
  return /windows/i.test(userAgent());
}

export function modLabel(): string {
  return isMac ? '⌘' : 'Ctrl';
}

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
