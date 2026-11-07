import { settings, saveSettings } from './settings.svelte';
import { type Mode, type PresetColors, type CustomPreset } from './theme';
import { addPreset, baseColors, customId, nextName, readableOn, removePreset, updatePreset } from './theme-edit';

export function createPreset(mode: Mode, label: string, from?: string): string {
  const id = customId();
  const colors = baseColors(settings, mode, from ?? (mode === 'dark' ? settings.darkPreset : settings.lightPreset));
  const preset: CustomPreset = { id, name: nextName(settings.customPresets, mode, label), mode, colors, accentFg: readableOn(colors.accent) };
  saveSettings({ customPresets: addPreset($state.snapshot(settings.customPresets), preset), ...(mode === 'dark' ? { darkPreset: id } : { lightPreset: id }) });
  return id;
}

export function editPreset(id: string, patch: { name?: string; colors?: Partial<PresetColors> }) {
  saveSettings({ customPresets: updatePreset($state.snapshot(settings.customPresets), id, patch) });
}

export function deletePreset(id: string) {
  saveSettings({
    customPresets: removePreset($state.snapshot(settings.customPresets), id),
    ...(settings.lightPreset === id ? { lightPreset: 'light' } : {}),
    ...(settings.darkPreset === id ? { darkPreset: 'dark' } : {}),
  });
}

export function selectPreset(mode: Mode, id: string) {
  saveSettings(mode === 'dark' ? { darkPreset: id } : { lightPreset: id });
}
