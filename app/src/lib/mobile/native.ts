import * as CMD from '../daemon/tauri-commands';
import { isTauri } from '../daemon/client';

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
  minimize: () => call<void>(CMD.mobileMinimize),
  systemDark: () => call<boolean | null>(CMD.mobileSystemDark),
  systemBars: (dark: boolean) => call<void>(CMD.mobileSystemBars, { dark }),
  clipboardImage: () => call<string | null>(CMD.mobileClipboardImage),
};
