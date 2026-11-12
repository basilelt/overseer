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
    const n = (id: string, text: string) => ({ id, text, createdAt: '', updatedAt: '' });
    const h = show([{ line: { id: 'ln_1' } as LocalLine['line'], local: { notes: [n('a', 'short'), n('b', 'one\ntwo'), n('c', 'x'.repeat(100)), n('d', 'y\n'.repeat(20))] } }]);
    expect([...h.matchAll(/<textarea rows="(\d+)"[^>]*aria-label="Sticky note"/g)].map((m) => m[1])).toEqual(['1', '2', '3', '8']);
  });

  it('explains an empty session and keeps the add button off until something is typed', () => {
    const h = show([{ line: { id: 'ln_1' } as LocalLine['line'] }]);
    expect(h).toContain('No notes on this session yet');
    expect(h).toMatch(/<button class="btn[^"]*"[^>]*disabled/);
  });

  it('locks a note still being saved', () => {
    const h = show([{ line: { id: 'ln_1' } as LocalLine['line'], local: { notes: [{ id: 'pending-x', text: 'wip', createdAt: '', updatedAt: '' }] } }]);
    expect(h).toMatch(/<textarea[^>]*disabled[^>]*>wip<\/textarea>/);
  });

  it('saves on blur or Ctrl/Cmd+Enter, adds with Ctrl/Cmd+Enter', () => {
    expect(src).toMatch(/onblur=\{\(\) => save\(n\.id\)\}/);
    expect(src).toContain('isSubmitChord(e)');
    expect(src).toMatch(/onkeydown=\{\(e\) => onKey\(e, add\)\}/);
    expect(src).toMatch(/onclick=\{\(\) => removeNote\(app, lineId, n\.id\)\}/);
  });

  it('forgets the open session when it is unmounted, so hiding the sidebar does not reopen the notes later', () => {
    expect(src).toMatch(/onDestroy\(\(\) => \{[^}]*closeNotes\(\);/);
  });

  it('keeps unsaved text when the notes close or switch session, since WebKit fires no blur on removal', () => {
    expect(src).toMatch(/onDestroy\(\(\) => \{\s*flush\(shown\);\s*closeNotes\(\);\s*\}\);/);
    expect(src).toMatch(/untrack\(\(\) => flush\(shown\)\);\s*shown = id;/);
    expect(src).toMatch(/for \(const \[id, t\] of pending\) void saveText\(line, id, t\);\s*if \(line && text\.trim\(\)\) void addNote\(app, line, text\);/);
    expect(src).toMatch(/!\(await editNote\(app, line, id, text\)\) && line === shown\) edits = \{ \.\.\.edits, \[id\]: text \}/);
  });

  it('is loaded only once a session opens its notes', () => {
    expect(sidebarSrc).toContain("const notesView = () => import('./NotesPopover.svelte');");
    expect(sidebarSrc).toMatch(/\{#if notesUi\.lineId\}\s*\{#await notesView\(\)/);
  });
});
