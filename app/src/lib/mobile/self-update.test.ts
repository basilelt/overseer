import { describe, expect, it } from 'vitest';
import { selfUpdate } from './self-update';

describe('self-update build flag', () => {
  it('keeps the APK updater in desktop, web and GitHub Android builds', () => {
    expect(selfUpdate({})).toBe(true);
    expect(selfUpdate({ TAURI_ENV_PLATFORM: 'android' })).toBe(true);
    expect(selfUpdate({ TAURI_ENV_PLATFORM: 'linux', OVERSEER_DISTRIBUTION: 'github' })).toBe(true);
  });

  it('leaves it out of the Google Play build', () => {
    expect(selfUpdate({ TAURI_ENV_PLATFORM: 'android', OVERSEER_DISTRIBUTION: 'play' })).toBe(false);
  });

  it('leaves it out of the iOS build', () => {
    expect(selfUpdate({ TAURI_ENV_PLATFORM: 'ios' })).toBe(false);
  });

  it('is compiled into this build as a constant', () => {
    expect(__SELF_UPDATE__).toBe(selfUpdate());
  });
});
