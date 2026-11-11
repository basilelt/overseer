import { describe, expect, it } from 'vitest';
import { mainOverlay } from './overlay-route';

const at = (overlay: string | null, more: Partial<{ overlayArg: string | null; hasFile: boolean; phone: boolean }> = {}) => mainOverlay({ overlay, hasFile: false, phone: false, ...more });

describe('mainOverlay', () => {
  it('shows nothing without an overlay or for an unknown one', () => {
    expect(at(null)).toBeNull();
    expect(at('nope')).toBeNull();
  });

  it('maps the plain overlays to themselves', () => {
    for (const o of ['palette', 'settings', 'task', 'taskDetail', 'project', 'review', 'broadcast', 'holdAll', 'name', 'workspaceNew', 'profiles', 'ports', 'system', 'usage', 'notifications', 'switchboardImport', 'uninstall']) expect(at(o)).toBe(o);
  });

  it('opens the phone connect screen on a phone and the dialog elsewhere', () => {
    expect(at('connect', { phone: true })).toBe('mobileConnect');
    expect(at('connect')).toBe('connect');
  });

  it('needs a file for the file viewer and a tab for the close tab dialog', () => {
    expect(at('file')).toBeNull();
    expect(at('file', { hasFile: true })).toBe('file');
    expect(at('closeTab')).toBeNull();
    expect(at('closeTab', { overlayArg: 'tab-1' })).toBe('closeTab');
  });

  it('never shows the shortcuts sheet on a phone', () => {
    expect(at('shortcuts')).toBe('shortcuts');
    expect(at('shortcuts', { phone: true })).toBeNull();
  });
});
