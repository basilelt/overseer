import { describe, expect, it } from 'vitest';
import { escapeTarget, mainOverlay, parentOverlay, type EscapeState } from './overlay-route';

describe('escapeTarget', () => {
  const idle: EscapeState = { key: 'Escape', defaultPrevented: false, feedback: false, recording: false, diag: false, overlay: false };

  it('closes the feedback sheet, which sits above every other dialog', () => {
    expect(escapeTarget({ ...idle, feedback: true, overlay: true, diag: true })).toBe('feedback');
  });

  it('leaves the sheet open when a popover inside it already took the key', () => {
    expect(escapeTarget({ ...idle, feedback: true, defaultPrevented: true })).toBeNull();
    expect(escapeTarget({ ...idle, feedback: true, overlay: true, defaultPrevented: true })).toBe('overlay');
  });

  it('leaves the sheet to the recording flow while one is being set up or running', () => {
    expect(escapeTarget({ ...idle, feedback: true, recording: true })).toBeNull();
  });

  it('closes the logs dialog before an overlay, and an overlay when nothing else is open', () => {
    expect(escapeTarget({ ...idle, diag: true, overlay: true })).toBe('diag');
    expect(escapeTarget({ ...idle, overlay: true })).toBe('overlay');
    expect(escapeTarget(idle)).toBeNull();
  });

  it('ignores other keys', () => {
    expect(escapeTarget({ ...idle, key: 'Enter', feedback: true, overlay: true })).toBeNull();
  });
});

describe('parentOverlay', () => {
  it('returns the uninstall dialog to settings', () => {
    expect(parentOverlay('uninstall')).toBe('settings');
  });

  it('has no parent for any other overlay or for none', () => {
    expect(parentOverlay('settings')).toBeNull();
    expect(parentOverlay('palette')).toBeNull();
    expect(parentOverlay('constructor')).toBeNull();
    expect(parentOverlay(null)).toBeNull();
  });
});

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
