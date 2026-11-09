import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { recorder } from './record.svelte';
import { MAX_EVENTS, MAX_MS, type Recording } from './record';
import { el, FakeEl } from '../test/fake-dom';

let win: EventTarget;

function fire(type: string, target: unknown, init: Record<string, unknown> = {}) {
  const ev = new Event(type);
  Object.defineProperty(ev, 'target', { value: target });
  for (const [k, v] of Object.entries(init)) Object.defineProperty(ev, k, { value: v });
  win.dispatchEvent(ev);
}

const keydown = (target: unknown, key: string, mods: Record<string, boolean> = {}) => fire('keydown', target, { key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });

class FakeTrack extends EventTarget {
  stop = vi.fn();
}

class FakeMediaRecorder extends EventTarget {
  static last: FakeMediaRecorder | null = null;
  static isTypeSupported = (m: string) => m === 'video/webm;codecs=vp8';
  state: 'inactive' | 'recording' = 'inactive';
  mimeType: string;
  ondataavailable: ((e: { data: Blob }) => void) | null = null;
  constructor(
    public stream: unknown,
    public opts: { mimeType?: string; videoBitsPerSecond?: number },
  ) {
    super();
    this.mimeType = opts.mimeType ?? '';
    FakeMediaRecorder.last = this;
  }
  start() {
    this.state = 'recording';
  }
  stop() {
    this.state = 'inactive';
    this.ondataavailable?.({ data: new Blob(['frames'], { type: this.mimeType }) });
    this.dispatchEvent(new Event('stop'));
  }
}

function stubMedia(fail?: Error) {
  const track = new FakeTrack();
  const getDisplayMedia = vi.fn(async () => {
    if (fail) throw fail;
    return { getVideoTracks: () => [track], getTracks: () => [track] };
  });
  vi.stubGlobal('navigator', { mediaDevices: { getDisplayMedia } });
  vi.stubGlobal('MediaRecorder', FakeMediaRecorder);
  return { track, getDisplayMedia };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date(2026, 0, 1, 12, 0, 0));
  win = new EventTarget();
  vi.stubGlobal('window', win);
  vi.stubGlobal('Element', FakeEl);
  FakeMediaRecorder.last = null;
});

afterEach(() => {
  recorder.cancel();
  recorder.dismiss();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

async function recording(video = false) {
  const done = vi.fn<(r: Recording) => void>();
  await recorder.start({ video, onDone: done });
  return done;
}

describe('recorder phases', () => {
  it('goes from idle to setup and back on dismiss', () => {
    expect(recorder.phase).toBe('idle');
    recorder.setup();
    expect(recorder.phase).toBe('setup');
    recorder.dismiss();
    expect(recorder.phase).toBe('idle');
  });

  it('does not open setup while recording', async () => {
    await recording();
    recorder.setup();
    expect(recorder.phase).toBe('recording');
  });

  it('ignores a second start while recording', async () => {
    const first = await recording();
    const second = vi.fn();
    await recorder.start({ video: false, onDone: second });
    await recorder.stop();
    expect(first).toHaveBeenCalledOnce();
    expect(second).not.toHaveBeenCalled();
  });
});

describe('recording actions', () => {
  it('logs the start, clicks with their time and hands the recording over on stop', async () => {
    const done = await recording();
    vi.advanceTimersByTime(2_500);
    fire('click', el('button', { 'aria-label': 'Open settings' }));
    vi.advanceTimersByTime(1_000);
    const rec = await recorder.stop();
    expect(rec?.events).toEqual([
      { at: 0, kind: 'start', text: 'Recording started' },
      { at: 2_500, kind: 'click', text: 'button "Open settings"' },
    ]);
    expect(rec?.duration).toBe(3_500);
    expect(rec?.video).toBeNull();
    expect(done).toHaveBeenCalledWith(rec);
    expect(recorder.phase).toBe('idle');
  });

  it('skips clicks on the recorder controls and on unnamed elements', async () => {
    await recording();
    fire('click', el('button', { title: 'Stop' }).in(el('div', { 'data-rec-skip': '' })));
    fire('click', el('div'));
    fire('click', {});
    const rec = await recorder.stop();
    expect(rec?.events.map((e) => e.kind)).toEqual(['start']);
  });

  it('logs shortcuts but never the characters typed, only where', async () => {
    await recording();
    const box = el('input', { type: 'text', placeholder: 'Search' });
    keydown(el('body'), 'p', { ctrlKey: true });
    for (const k of 'hunter2') keydown(box, k);
    keydown(box, 'Enter');
    const rec = await recorder.stop();
    expect(rec?.events.slice(1).map((e) => [e.kind, e.text])).toEqual([
      ['key', 'Ctrl+P'],
      ['type', 'typed in input "Search"'],
    ]);
    expect(JSON.stringify(rec)).not.toContain('hunter');
  });

  it('logs typing again after focus moves to another field', async () => {
    await recording();
    const a = el('textarea', { 'aria-label': 'A' });
    const b = el('textarea', { 'aria-label': 'B' });
    keydown(a, 'x');
    fire('focusin', b);
    keydown(b, 'y');
    fire('focusin', a);
    keydown(a, 'z');
    const rec = await recorder.stop();
    expect(rec?.events.filter((e) => e.kind === 'type').length).toBe(3);
  });

  it('logs uncaught errors and rejections', async () => {
    await recording();
    fire('error', win, { message: 'boom', error: null });
    fire('unhandledrejection', win, { reason: new Error('nope') });
    const rec = await recorder.stop();
    expect(rec?.events.slice(1).map((e) => [e.kind, e.text])).toEqual([
      ['error', 'boom'],
      ['error', 'nope'],
    ]);
  });

  it('collapses repeated identical notes but keeps repeated clicks', async () => {
    await recording();
    recorder.note('view', 'view kanban');
    recorder.note('view', 'view kanban');
    const btn = el('button', { title: 'Next' });
    fire('click', btn);
    fire('click', btn);
    const rec = await recorder.stop();
    expect(rec?.events.map((e) => e.kind)).toEqual(['start', 'view', 'click', 'click']);
  });

  it('caps the number of events', async () => {
    await recording();
    for (let i = 0; i < MAX_EVENTS + 50; i++) recorder.note('toast', `e${i}`);
    expect(recorder.count).toBe(MAX_EVENTS);
  });

  it('ignores notes while idle', () => {
    recorder.note('toast', 'x');
    expect(recorder.count).toBe(0);
  });

  it('updates the elapsed time every second and stops on its own at the limit', async () => {
    const done = await recording();
    vi.advanceTimersByTime(3_000);
    expect(recorder.elapsed).toBe(3_000);
    await vi.advanceTimersByTimeAsync(MAX_MS);
    expect(done).toHaveBeenCalledOnce();
    expect(done.mock.calls[0][0].duration).toBe(MAX_MS);
    expect(recorder.phase).toBe('idle');
  });

  it('removes every listener and timer on stop', async () => {
    await recording();
    await recorder.stop();
    fire('click', el('button', { title: 'After' }));
    expect(vi.getTimerCount()).toBe(0);
    expect(recorder.count).toBe(0);
  });

  it('ignores a discard while a stop is still flushing the video', async () => {
    stubMedia();
    const done = await recording(true);
    const stopping = recorder.stop();
    recorder.cancel();
    const rec = await stopping;
    expect(done).toHaveBeenCalledWith(rec);
    expect(await rec?.video?.text()).toBe('frames');
  });

  it('discards everything on cancel without calling back', async () => {
    const done = await recording();
    fire('click', el('button', { title: 'X' }));
    recorder.cancel();
    expect(recorder.phase).toBe('idle');
    expect(vi.getTimerCount()).toBe(0);
    expect(await recorder.stop()).toBeNull();
    expect(done).not.toHaveBeenCalled();
  });
});

describe('screen video', () => {
  it('records the shared screen and returns the video with its format', async () => {
    const { getDisplayMedia, track } = stubMedia();
    await recording(true);
    expect(getDisplayMedia).toHaveBeenCalledOnce();
    expect(FakeMediaRecorder.last?.opts).toEqual({ mimeType: 'video/webm;codecs=vp8', videoBitsPerSecond: 600_000 });
    const rec = await recorder.stop();
    expect(rec?.events[0].text).toBe('Recording started with screen video');
    expect(rec?.mime).toBe('video/webm;codecs=vp8');
    expect(await rec?.video?.text()).toBe('frames');
    expect(track.stop).toHaveBeenCalled();
  });

  it('stops when the user ends the share from the browser', async () => {
    const { track } = stubMedia();
    const done = await recording(true);
    track.dispatchEvent(new Event('ended'));
    await vi.waitFor(() => expect(done).toHaveBeenCalledOnce());
    expect(recorder.phase).toBe('idle');
  });

  it('goes back to idle and rethrows when the share is refused', async () => {
    stubMedia(new Error('Permission denied'));
    await expect(recording(true)).rejects.toThrow('Permission denied');
    expect(recorder.phase).toBe('idle');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('stops the screen share on cancel', async () => {
    const { track } = stubMedia();
    await recording(true);
    recorder.cancel();
    expect(track.stop).toHaveBeenCalled();
    expect(FakeMediaRecorder.last?.state).toBe('inactive');
  });
});
