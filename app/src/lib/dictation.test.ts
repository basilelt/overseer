import { describe, expect, it } from 'vitest';
import { dictationHintKey, dictationMode } from './dictation';

describe('dictationMode', () => {
  it('starts macOS dictation natively in the app and hints elsewhere', () => {
    expect(dictationMode('macos', 'MacIntel', true, false)).toBe('native');
    expect(dictationMode('', 'MacIntel', false, false)).toBe('hint');
    expect(dictationMode('windows', 'Win32', true, false)).toBe('hint');
  });

  it('hides the button where the OS has no dictation or the keyboard has its own mic', () => {
    expect(dictationMode('linux', 'Linux x86_64', true, false)).toBeNull();
    expect(dictationMode('macos', 'MacIntel', true, true)).toBeNull();
    expect(dictationMode('ios', 'MacIntel', true, false)).toBeNull();
  });

  it('picks the hint for the platform', () => {
    expect(dictationHintKey('MacIntel')).toBe('dictation.hintMac');
    expect(dictationHintKey('Win32')).toBe('dictation.hintWindows');
  });
});
