import { describe, expect, it, vi } from 'vitest';
import { DaemonError, type DaemonClient } from './daemon/client';
import { addNote, cleanNote, editNote, NOTE_MAX, NOTES_MAX, notesApi, notesOf, removeNote, withNotes } from './notes';
import type { DaemonEvent, LocalLine, Note } from './types';
import type { Snapshot } from './reduce';
import { quietMock } from '../test/quiet-mock';

const mock = () => quietMock();

async function host(client: DaemonClient | null = mock()) {
  const lines: LocalLine[] = client ? await client.request<LocalLine[]>('GET', '/v1/lines') : [{ line: { id: 'ln_01' } as LocalLine['line'] }];
  return { client, data: { lines } as Snapshot, notify: vi.fn() };
}

const fail = (status: number, msg: string) => ({ request: vi.fn(async () => Promise.reject(new DaemonError(status, msg))) }) as unknown as DaemonClient;

describe('cleanNote', () => {
  it('keeps line breaks and tabs, drops other control characters, trims and caps like the daemon', () => {
    expect(cleanNote('  todo:\n- a\u0007\n\tb  ')).toBe('todo:\n- a\n\tb');
    expect(cleanNote('a\r\nb')).toBe('a\nb');
    expect(cleanNote(' \n ')).toBeNull();
    expect(cleanNote('x'.repeat(NOTE_MAX + 5))).toHaveLength(NOTE_MAX);
  });
});

describe('notesApi on the mock daemon', () => {
  it('sends the daemon routes with an encoded line id and note id', async () => {
    const request = vi.fn(async () => ({ lineId: 'a/b', note: null, notes: [] }));
    const api = notesApi({ request } as unknown as DaemonClient);
    await api.add('a/b', 'x');
    await api.edit('a/b', 'n 1', 'y');
    await api.remove('a/b', 'n 1');
    expect(request.mock.calls).toEqual([
      ['POST', '/v1/lines/a%2Fb/notes', { text: 'x' }],
      ['PUT', '/v1/lines/a%2Fb/notes/n%201', { text: 'y' }],
      ['DELETE', '/v1/lines/a%2Fb/notes/n%201'],
    ]);
  });

  it('rejects what the daemon rejects, with the same codes and messages', async () => {
    const api = notesApi(mock());
    const err = (p: Promise<unknown>) => p.then(() => null, (e: DaemonError) => [e.status, e.message]);
    expect(await err(api.add('ln_01', '  '))).toEqual([400, 'note text required']);
    expect(await err(mock().request('POST', '/v1/lines/ln_01/notes', {}))).toEqual([400, 'text (string) required']);
    expect(await err(api.add('nope', 'x'))).toEqual([404, "no line matches 'nope'"]);
    expect(await err(api.edit('ln_01', 'nope', 'x'))).toEqual([404, 'no note nope']);
    expect(await err(api.remove('ln_01', 'nope'))).toEqual([404, 'no note nope']);
    expect(await err(mock().request('GET', '/v1/lines/ln_01/notes'))).toEqual([404, 'not found']);
  });

  it('caps a session at the daemon limit', async () => {
    const api = notesApi(mock());
    for (let i = 0; i < NOTES_MAX; i++) await api.add('ln_01', `n${i}`);
    await expect(api.add('ln_01', 'one more')).rejects.toMatchObject({ status: 409 });
  });

  it('announces each change as a line update carrying the notes', async () => {
    const m = mock();
    const seen: DaemonEvent[] = [];
    m.subscribe((e) => seen.push(e));
    const r = await notesApi(m).add('ln_01', 'hello');
    expect(seen.at(-1)).toMatchObject({ type: 'line.updated', data: { line: { id: 'ln_01' }, local: { notes: r.notes } } });
  });
});

describe('note actions', () => {
  it('adds, edits and removes a note and keeps the daemon list', async () => {
    const app = await host();
    expect(await addNote(app, 'ln_01', '  call back  ')).toBe(true);
    const [n] = notesOf(app.data, 'ln_01');
    expect(n.text).toBe('call back');
    expect(n.id).not.toMatch(/^pending-/);
    expect(await editNote(app, 'ln_01', n.id, 'call back tomorrow')).toBe(true);
    expect(notesOf(app.data, 'ln_01')[0].text).toBe('call back tomorrow');
    expect(await removeNote(app, 'ln_01', n.id)).toBe(true);
    expect(notesOf(app.data, 'ln_01')).toEqual([]);
    expect(app.notify).not.toHaveBeenCalled();
  });

  it('ignores a blank new note without calling the daemon', async () => {
    const client = fail(500, 'boom');
    const app = await host(null);
    app.client = client;
    expect(await addNote(app, 'ln_01', '   ')).toBe(false);
    expect(client.request).not.toHaveBeenCalled();
  });

  it('deletes a note whose text was cleared, and skips an unchanged edit', async () => {
    const app = await host();
    await addNote(app, 'ln_01', 'a');
    const id = notesOf(app.data, 'ln_01')[0].id;
    const spy = vi.spyOn(app.client!, 'request');
    expect(await editNote(app, 'ln_01', id, ' a ')).toBe(true);
    expect(spy).not.toHaveBeenCalled();
    await editNote(app, 'ln_01', id, '  ');
    expect(spy).toHaveBeenCalledWith('DELETE', `/v1/lines/ln_01/notes/${id}`);
    expect(notesOf(app.data, 'ln_01')).toEqual([]);
  });

  it('shows the change at once and puts the old list back when the daemon refuses it', async () => {
    const app = await host(null);
    const old: Note = { id: 'n1', text: 'keep', createdAt: 't', updatedAt: 't' };
    app.data = withNotes(app.data, 'ln_01', [old]);
    let release!: (v: unknown) => void;
    app.client = { request: vi.fn(() => new Promise((_, rej) => (release = rej))) } as unknown as DaemonClient;
    const p = removeNote(app, 'ln_01', 'n1');
    expect(notesOf(app.data, 'ln_01')).toEqual([]);
    release(new DaemonError(404, 'no line matches ln_01'));
    expect(await p).toBe(false);
    expect(notesOf(app.data, 'ln_01')).toEqual([old]);
    expect(app.notify).toHaveBeenCalledWith('no line matches ln_01', 'error');
  });

  it('reports a lost connection instead of throwing', async () => {
    const app = await host(null);
    expect(await addNote(app, 'ln_01', 'x')).toBe(false);
    expect(app.notify).toHaveBeenCalledWith('not connected', 'error');
    expect(notesOf(app.data, 'ln_01')).toEqual([]);
  });

  it('leaves other sessions untouched', () => {
    const snap = { lines: [{ line: { id: 'a' } }, { line: { id: 'b' }, local: { pinned: true } }] as LocalLine[] };
    const next = withNotes(snap, 'b', [{ id: 'n', text: 't', createdAt: '', updatedAt: '' }]);
    expect(next.lines[0]).toBe(snap.lines[0]);
    expect(next.lines[1].local).toMatchObject({ pinned: true, notes: [{ id: 'n' }] });
    expect(notesOf(snap, 'missing')).toEqual([]);
  });
});
