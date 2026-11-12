import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { app } from '../../state.svelte';
import { closeNotes, toggleNotes } from '../../notes.svelte';
import type { LocalLine } from '../../types';
import NotesPopover from './NotesPopover.svelte';
import src from './NotesPopover.svelte?raw';
import sidebarSrc from './Sidebar.svelte?raw';

const saved = app.data;
afterEach(() => {
  app.data = saved;
  closeNotes();
});

function show(lines: LocalLine[]) {
  app.data = { ...app.data, lines };
  toggleNotes('ln_1', { right: 100, top: 10 });
  return render(NotesPopover).body;
}

describe('NotesPopover', () => {
  it('lists the notes of the open session, each editable and deletable', () => {
    // Given a session with two notes
    // When its notes are open
    // Then both texts are in editable boxes with a delete button, and the add box is there
    const h = show([{ line: { id: 'ln_1' } as LocalLine['line'], local: { notes: [
      { id: 'a', text: 'ask about CI', createdAt: '2026-11-05T10:00:00Z', updatedAt: '2026-11-05T10:00:00Z' },
      { id: 'b', text: 'merge after Thursday', createdAt: '2026-11-05T10:00:00Z', updatedAt: '2026-11-05T10:00:00Z' },
    ] } }]);
    expect(h).toContain('role="dialog"');
    expect(h).toContain('aria-label="Sticky notes"');
    expect(h).toMatch(/<textarea[^>]*aria-label="Sticky note"[^>]*>ask about CI<\/textarea>/);
    expect(h).toContain('>merge after Thursday</textarea>');
    expect(h.match(/aria-label="Delete note"/g)).toHaveLength(2);
    expect(h).toContain('aria-label="Add a sticky note"');
    expect(h).not.toContain('No notes on this session yet');
  });

  it('sizes each note to its lines, wrapped ones included, up to eight rows', () => {
    // Given a one-line note, a two-line note, a long line and a very long note
    // When the notes are open
    // Then each box gets as many rows as its text needs, capped at eight
    const n = (id: string, text: string) => ({ id, text, createdAt: '', updatedAt: '' });
    const h = show([{ line: { id: 'ln_1' } as LocalLine['line'], local: { notes: [n('a', 'short'), n('b', 'one\ntwo'), n('c', 'x'.repeat(100)), n('d', 'y\n'.repeat(20))] } }]);
    expect([...h.matchAll(/<textarea rows="(\d+)"[^>]*aria-label="Sticky note"/g)].map((m) => m[1])).toEqual(['1', '2', '3', '8']);
  });

  it('explains an empty session and keeps the add button off until something is typed', () => {
    // Given a session without notes
    // When its notes are open
    // Then the empty text shows and the add button is disabled
    const h = show([{ line: { id: 'ln_1' } as LocalLine['line'] }]);
    expect(h).toContain('No notes on this session yet');
    expect(h).toMatch(/<button class="btn[^"]*"[^>]*disabled/);
  });

  it('locks a note still being saved', () => {
    // Given a note added a moment ago that the daemon has not confirmed
    // When its notes are open
    // Then its box and delete button are disabled
    const h = show([{ line: { id: 'ln_1' } as LocalLine['line'], local: { notes: [{ id: 'pending-x', text: 'wip', createdAt: '', updatedAt: '' }] } }]);
    expect(h).toMatch(/<textarea[^>]*disabled[^>]*>wip<\/textarea>/);
  });

  it('saves on blur or Ctrl/Cmd+Enter, adds with Ctrl/Cmd+Enter, and closes on Escape or a click outside', () => {
    // Given the component source
    // When the handlers are read
    // Then they reach the note actions and close without eating the button that toggles it
    expect(src).toMatch(/onblur=\{\(\) => save\(n\.id\)\}/);
    expect(src).toContain('isSubmitChord(e)');
    expect(src).toMatch(/onkeydown=\{\(e\) => onKey\(e, add\)\}/);
    expect(src).toContain("use:dismiss={{ onclose: closeNotes, escape: 'stop', keep: trigger }}");
    expect(src).toMatch(/el\?\.closest\?\.\(`\[data-notes=/);
    expect(src).toMatch(/onclick=\{\(\) => removeNote\(app, lineId, n\.id\)\}/);
  });

  it('forgets the open session when it is unmounted, so hiding the sidebar does not reopen the notes later', () => {
    expect(src).toMatch(/onDestroy\(\(\) => \{[^}]*closeNotes\(\);/);
  });

  it('keeps unsaved text when the notes close or switch session, since WebKit fires no blur on removal', () => {
    // Given the component source
    // When closing and switching are read
    // Then pending edits and the new-note draft are saved for the session that was shown
    expect(src).toMatch(/onDestroy\(\(\) => \{\s*flush\(shown\);\s*closeNotes\(\);\s*\}\);/);
    expect(src).toMatch(/untrack\(\(\) => flush\(shown\)\);\s*shown = id;/);
    expect(src).toMatch(/for \(const \[id, t\] of pending\) void saveText\(line, id, t\);\s*if \(line && text\.trim\(\)\) void addNote\(app, line, text\);/);
    expect(src).toMatch(/!\(await editNote\(app, line, id, text\)\) && line === shown\) edits = \{ \.\.\.edits, \[id\]: text \}/);
  });

  it('is loaded only once a session opens its notes', () => {
    // Given the sidebar source
    // Then the popover is a lazy import behind notesUi.lineId
    expect(sidebarSrc).toContain("const notesView = () => import('./NotesPopover.svelte');");
    expect(sidebarSrc).toMatch(/\{#if notesUi\.lineId\}\s*\{#await notesView\(\)/);
  });
});
