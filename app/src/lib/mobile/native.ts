import { isTauri } from '../client';

let android = false;

export function setNativeOs(os: string) {
  android = os === 'android' && isTauri();
}

export function isNativeAndroid(): boolean {
  return android;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!android) return null;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

export const native = {
  minimize: () => call<void>('mobile_minimize'),
  systemDark: () => call<boolean | null>('mobile_system_dark'),
  systemBars: (dark: boolean) => call<void>('mobile_system_bars', { dark }),
  clipboardImage: () => call<string | null>('mobile_clipboard_image'),
};
