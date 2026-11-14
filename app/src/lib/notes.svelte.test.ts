import { afterEach, describe, expect, it } from 'vitest';
import { closeNotes, notesUi, toggleNotes } from './notes.svelte';

afterEach(() => closeNotes());

describe('notesUi', () => {
  it('opens next to the clicked control, then the same control closes it', () => {
    toggleNotes('ln_01', { right: 200, top: 40 });
    expect({ ...notesUi }).toEqual({ lineId: 'ln_01', x: 206, y: 40 });
    toggleNotes('ln_01', { right: 200, top: 40 });
    expect(notesUi.lineId).toBeNull();
  });

  it('switches to another session without closing', () => {
    toggleNotes('ln_01', { right: 200, top: 40 });
    toggleNotes('ln_02', { right: 200, top: 80 });
    expect({ ...notesUi }).toEqual({ lineId: 'ln_02', x: 206, y: 80 });
  });
});
