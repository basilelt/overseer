import { describe, expect, it, vi } from 'vitest';
import { SelectionManager } from 'ghostty-web';
import { finishSelectionOnInterruption, routeCopies } from './selection-copy';

function fakeManager() {
  const native = vi.fn();
  return { native, sm: { copyToClipboard: native as (text: string) => void } };
}

describe('routeCopies', () => {
  it('sends the copied selection to the app clipboard instead of navigator.clipboard', () => {
    const { native, sm } = fakeManager();
    const copy = vi.fn(async () => {});
    routeCopies(sm, copy);
    sm.copyToClipboard('hello');
    expect(copy).toHaveBeenCalledWith('hello');
    expect(native).not.toHaveBeenCalled();
  });

  it('ignores an empty selection', () => {
    const { sm } = fakeManager();
    const copy = vi.fn(async () => {});
    routeCopies(sm, copy);
    sm.copyToClipboard('');
    expect(copy).not.toHaveBeenCalled();
  });

  it('swallows a rejected copy', async () => {
    const { sm } = fakeManager();
    const copy = vi.fn(() => Promise.reject(new Error('denied')));
    routeCopies(sm, copy);
    sm.copyToClipboard('x');
    await Promise.resolve();
    expect(copy).toHaveBeenCalledOnce();
  });

  it('leaves a terminal without a selection manager alone', () => {
    expect(() => routeCopies(undefined, async () => {})).not.toThrow();
    expect(() => routeCopies({}, async () => {})).not.toThrow();
  });

  it('catches the copy of the real ghostty-web selection manager, used by Cmd+C and mouse selection', () => {
    const sm = Object.create(SelectionManager.prototype) as { copySelection(): boolean };
    Object.assign(sm, { hasSelection: () => true, getSelection: () => 'from ghostty' });
    const copy = vi.fn(async () => {});
    routeCopies(sm as never, copy);
    expect(sm.copySelection()).toBe(true);
    expect(copy).toHaveBeenCalledWith('from ghostty');
  });
});

describe('finishSelectionOnInterruption', () => {
  it('finishes terminal selection when blur or pointer cancellation skips mouseup', () => {
    const win = new EventTarget();
    const terminal = new EventTarget();
    const doc = new EventTarget();
    const received: string[] = [];
    doc.addEventListener('mouseup', () => received.push('mouseup'));
    const stop = finishSelectionOnInterruption(win, terminal, doc);

    win.dispatchEvent(new Event('blur'));
    terminal.dispatchEvent(new Event('pointercancel'));

    expect(received).toEqual(['mouseup', 'mouseup']);
    stop();
    win.dispatchEvent(new Event('blur'));
    expect(received).toHaveLength(2);
  });
});
