import { afterEach, describe, expect, it } from 'vitest';
import { holdPopup, popup } from './popup.svelte';
import { diag } from './diag/diag.svelte';
import { feedback } from './feedback/feedback.svelte';
import { ui } from './mobile/ui.svelte';
import { rec } from './record/record-state.svelte';
import { app } from './state.svelte';
import { tour } from './tour/tour.svelte';
import { organizer } from './workspace/organize.svelte';

afterEach(() => {
  app.overlay = null;
  app.menu = null;
  feedback.close();
  diag.mode = null;
  rec.phase = 'idle';
  organizer.phase = null;
  ui.drawer = false;
  tour.phase = null;
});

describe('popup.open', () => {
  it('is false when no popup is open', () => {
    expect(popup.open).toBe(false);
  });

  it('is true while a main overlay is open', () => {
    app.overlay = 'settings';
    expect(popup.open).toBe(true);
  });

  it('is true while a menu is open', () => {
    app.menu = { items: [], x: 0, y: 0, label: '' };
    expect(popup.open).toBe(true);
  });

  it('is true while the feedback sheet is open', () => {
    feedback.show();
    expect(popup.open).toBe(true);
  });

  it('is true while the diagnostics dialog is open', () => {
    diag.mode = 'live';
    expect(popup.open).toBe(true);
  });

  it('is true while the record setup dialog is open', () => {
    rec.phase = 'setup';
    expect(popup.open).toBe(true);
  });

  it('is false while only the recording pill floats', () => {
    rec.phase = 'recording';
    expect(popup.open).toBe(false);
  });

  it('is true while an organize dialog is open', () => {
    organizer.phase = 'review';
    expect(popup.open).toBe(true);
  });

  it('is true while the mobile drawer is open', () => {
    ui.drawer = true;
    expect(popup.open).toBe(true);
  });

  it('is true while the tour is running', () => {
    tour.phase = 'tour';
    expect(popup.open).toBe(true);
  });

  it('goes back to false when the popup closes', () => {
    feedback.show();
    expect(popup.open).toBe(true);
    feedback.close();
    expect(popup.open).toBe(false);
  });
});

describe('holdPopup', () => {
  it('keeps popup.open true until every holder released', () => {
    const a = holdPopup();
    const b = holdPopup();
    expect(popup.open).toBe(true);
    a();
    expect(popup.open).toBe(true);
    b();
    expect(popup.open).toBe(false);
  });
});

const sources = import.meta.glob('./**/*.svelte', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
const SURFACE = /position:\s*fixed|role="(dialog|menu|alertdialog)"/;
const COVERED_ELSEWHERE = new Set([
  'CommandPalette',
  'ConnectDialog',
  'MobileConnect',
  'MobileShell',
  'PanelShell',
  'SourceControl',
  'Tour',
]);
const NOT_A_POPUP = new Set(['AuthPrompt', 'Boundary', 'KanbanView', 'RecordPill', 'Toast']);

describe('floating surfaces', () => {
  it('every component that floats above the page holds the native webviews hidden, or is listed with its reason', () => {
    const missing = Object.entries(sources)
      .filter(([, src]) => SURFACE.test(src))
      .map(([path]) => ({ path, name: path.split('/').pop()!.replace('.svelte', '') }))
      .filter(({ path, name }) => !/holdPopup\)|holdPopup\(\)/.test(sources[path]) && !COVERED_ELSEWHERE.has(name) && !NOT_A_POPUP.has(name))
      .map(({ name }) => name);
    expect(missing).toEqual([]);
  });
});
