import * as CMD from './tauri-commands';
export type DictationMode = 'native' | 'hint' | null;

export function dictationMode(os: string, platform: string, tauri: boolean, phone: boolean): DictationMode {
  if (phone) return null;
  if (tauri) return os === 'macos' ? 'native' : os === 'windows' ? 'hint' : null;
  return /mac|win/i.test(platform) ? 'hint' : null;
}

export function dictationHintKey(platform: string): 'dictation.hintMac' | 'dictation.hintWindows' {
  return /mac/i.test(platform) ? 'dictation.hintMac' : 'dictation.hintWindows';
}

export async function startDictation(): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke(CMD.startDictation);
}
