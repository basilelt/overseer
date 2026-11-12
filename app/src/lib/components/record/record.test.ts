import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Recording } from '../../record/record';

function clean(html: string): string {
  for (let prev = ''; prev !== html; ) {
    prev = html;
    html = html.replace(/<!--[\s\S]*?--!?>/g, '');
  }
  return html;
}

async function render(path: 'Replay' | 'RecordPill' | 'RecordSetup', props: Record<string, unknown> = {}) {
  const { render } = await import('svelte/server');
  const mod = path === 'Replay' ? await import('./Replay.svelte') : path === 'RecordPill' ? await import('./RecordPill.svelte') : await import('./RecordSetup.svelte');
  return clean(render(mod.default as never, { props } as never).body);
}

const rec = (over: Partial<Recording> = {}): Recording => ({
  events: [
    { at: 0, kind: 'start', text: 'Recording started' },
    { at: 4_000, kind: 'click', text: 'button "Save"' },
    { at: 7_000, kind: 'error', text: 'boom' },
  ],
  duration: 9_000,
  video: null,
  mime: '',
  ...over,
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
});

describe('Replay', () => {
  it('lists every action with its time as a seek button', async () => {
    const html = await render('Replay', { recording: rec() });
    expect(html).toMatch(/<ol class="events[^"]*" aria-label="Actions"/);
    expect(html.match(/<button/g)?.length).toBe(3);
    expect(html).toContain('0:04');
    expect(html).toContain('button "Save"');
    expect(html).toMatch(/kind-error[^>]*>error</);
  });

  it('explains there is no video when the recording has none', async () => {
    const html = await render('Replay', { recording: rec() });
    expect(html).toContain('No video for this recording');
    expect(html).not.toContain('<video');
  });
});

describe('RecordPill', () => {
  it('shows the elapsed time, the action count, Stop and Discard, and is skipped by the recorder', async () => {
    const html = await render('RecordPill');
    expect(html).toContain('data-rec-skip');
    expect(html).toContain('Recording 0:00');
    expect(html).toContain('0 actions');
    expect(html).toMatch(/<button[^>]*>.*Stop<\/button>/s);
    expect(html).toContain('aria-label="Discard recording"');
  });
});

describe('RecordSetup', () => {
  it('offers the screen video when the browser can record the screen', async () => {
    vi.stubGlobal('navigator', { mediaDevices: { getDisplayMedia: () => {} } });
    vi.stubGlobal('MediaRecorder', class {});
    const html = await render('RecordSetup');
    expect(html).toContain('Record the screen too');
    expect(html).toContain('Recordings stop after 2 minutes');
    expect(html).toContain('Start recording');
    expect(html).not.toContain('Screen recording is not available');
  });

  it('falls back to the action log alone where screen recording is missing', async () => {
    vi.stubGlobal('navigator', { mediaDevices: undefined });
    const html = await render('RecordSetup');
    expect(html).toContain('Screen recording is not available here');
    expect(html).not.toContain('Record the screen too');
  });

  it('says typed text is never logged', async () => {
    expect(await render('RecordSetup')).toContain('What you type is never logged');
  });
});

describe('entry points', () => {
  it('lists Record a bug in the palette Actions while idle only', async () => {
    const { render } = await import('svelte/server');
    const { recorder } = await import('../../record/record.svelte');
    const { default: Palette } = await import('../overlays/CommandPalette.svelte');
    expect(render(Palette, { props: { onclose: () => {} } }).body).toContain('Record a bug');
    recorder.setup();
    expect(render(Palette, { props: { onclose: () => {} } }).body).not.toContain('Record a bug');
    recorder.dismiss();
  });

  it('lists Record a bug in the phone More menu, disabled while a recording runs', async () => {
    const { render: ssr } = await import('svelte/server');
    const { default: More } = await import('../mobile/MobileMore.svelte');
    const row = () => clean(ssr(More, { props: {} }).body).match(/<button class="row[^"]*"([^>]*)>(?:(?!<\/button>)[\s\S])*Record a bug/)?.[1];
    const { recorder } = await import('../../record/record.svelte');
    expect(row()).not.toContain('disabled');
    recorder.setup();
    expect(row()).toContain('disabled');
    recorder.dismiss();
  });
});

describe('feedback sheet with a recording', () => {
  it('shows the recording with its length, action count and a Replay toggle', async () => {
    const { feedback } = await import('../../feedback/feedback.svelte');
    feedback.show({ kind: 'bug', recording: rec() });
    const { render } = await import('svelte/server');
    const { default: Sheet } = await import('../overlays/FeedbackSheet.svelte');
    const html = clean(render(Sheet).body);
    feedback.close();
    expect(html).toContain('Recording');
    expect(html).toContain('0:09');
    expect(html).toContain('3 actions');
    expect(html).toMatch(/aria-expanded="false"[^>]*>Replay</);
  });

  it('offers to attach the video again when it is not attached yet', async () => {
    const { feedback } = await import('../../feedback/feedback.svelte');
    feedback.show({ kind: 'bug', recording: rec({ video: new Blob(['v'], { type: 'video/webm' }), mime: 'video/webm' }) });
    const { render } = await import('svelte/server');
    const { default: Sheet } = await import('../overlays/FeedbackSheet.svelte');
    const html = clean(render(Sheet).body);
    feedback.close();
    expect(html).toContain('>Attach video<');
  });

  it('has nothing to attach for a recording without video', async () => {
    const { feedback } = await import('../../feedback/feedback.svelte');
    feedback.show({ kind: 'bug', recording: rec() });
    const { render } = await import('svelte/server');
    const { default: Sheet } = await import('../overlays/FeedbackSheet.svelte');
    const html = clean(render(Sheet).body);
    feedback.close();
    expect(html).not.toContain('Attach video');
  });

  it('shows no recording block for a plain report', async () => {
    const { feedback } = await import('../../feedback/feedback.svelte');
    feedback.show();
    const { render } = await import('svelte/server');
    const { default: Sheet } = await import('../overlays/FeedbackSheet.svelte');
    const html = clean(render(Sheet).body);
    feedback.close();
    expect(html).not.toContain('>Replay<');
  });
});
