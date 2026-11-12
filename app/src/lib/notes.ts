import type { DaemonClient } from './daemon/client';
import type { Snapshot } from './reduce';
import type { Note } from './types';

export const NOTE_MAX = 4000;
export const NOTES_MAX = 50;

const enc = encodeURIComponent;

/** Same as the daemon's clean_note: newlines and tabs kept, other control characters dropped, trimmed and capped. */
export function cleanNote(raw: string): string | null {
  // eslint-disable-next-line no-control-regex
  const s = raw.replace(/[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g, '').trim();
  return Array.from(s).slice(0, NOTE_MAX).join('').trimEnd() || null;
}

export interface NotesReply {
  lineId: string;
  note: Note | null;
  notes: Note[];
}

export function notesApi(c: DaemonClient) {
  const base = (lineId: string) => `/v1/lines/${enc(lineId)}/notes`;
  return {
    add: (lineId: string, text: string) => c.request<NotesReply>('POST', base(lineId), { text }),
    edit: (lineId: string, id: string, text: string) => c.request<NotesReply>('PUT', `${base(lineId)}/${enc(id)}`, { text }),
    remove: (lineId: string, id: string) => c.request<NotesReply>('DELETE', `${base(lineId)}/${enc(id)}`),
  };
}

export function withNotes<S extends Pick<Snapshot, 'lines'>>(snap: S, lineId: string, notes: Note[]): S {
  return { ...snap, lines: snap.lines.map((l) => (l.line.id === lineId ? { ...l, local: { ...l.local, notes } } : l)) };
}

export function notesOf(snap: Pick<Snapshot, 'lines'>, lineId: string): Note[] {
  return snap.lines.find((l) => l.line.id === lineId)?.local?.notes ?? [];
}

interface Host {
  client: DaemonClient | null;
  data: Snapshot;
  notify: (msg: string, kind?: 'error') => void;
}

/** Runs one note change: optimistic `local` view, then the daemon's list, or the previous list back on failure. */
async function change(app: Host, lineId: string, local: (prev: Note[]) => Note[], call: (api: ReturnType<typeof notesApi>) => Promise<NotesReply>): Promise<boolean> {
  const prev = notesOf(app.data, lineId);
  app.data = withNotes(app.data, lineId, local(prev));
  try {
    if (!app.client) throw new Error('not connected');
    const r = await call(notesApi(app.client));
    app.data = withNotes(app.data, lineId, r.notes);
    return true;
  } catch (e) {
    app.data = withNotes(app.data, lineId, prev);
    app.notify(String((e as Error)?.message ?? e), 'error');
    return false;
  }
}

export function addNote(app: Host, lineId: string, raw: string): Promise<boolean> {
  const text = cleanNote(raw);
  if (!text) return Promise.resolve(false);
  const now = new Date().toISOString();
  const draft: Note = { id: `pending-${now}`, text, createdAt: now, updatedAt: now };
  return change(app, lineId, (p) => [...p, draft], (api) => api.add(lineId, text));
}

/** Saves an edited note; a blank text deletes it, an unchanged one does nothing. */
export function editNote(app: Host, lineId: string, id: string, raw: string): Promise<boolean> {
  const text = cleanNote(raw);
  if (!text) return removeNote(app, lineId, id);
  if (notesOf(app.data, lineId).find((n) => n.id === id)?.text === text) return Promise.resolve(true);
  return change(app, lineId, (p) => p.map((n) => (n.id === id ? { ...n, text } : n)), (api) => api.edit(lineId, id, text));
}

export function removeNote(app: Host, lineId: string, id: string): Promise<boolean> {
  return change(app, lineId, (p) => p.filter((n) => n.id !== id), (api) => api.remove(lineId, id));
}
