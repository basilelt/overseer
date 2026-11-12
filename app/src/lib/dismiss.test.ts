import { beforeEach, describe, expect, it, vi } from 'vitest';
import { eventTarget } from '../test/fake-events';
import { dismiss, type DismissOptions } from './dismiss';

const win = eventTarget();

const inside = {} as HTMLElement;
const node = { contains: (t: unknown) => t === inside } as unknown as HTMLElement;

function pointerdown(target: unknown = {}) {
  win.fire('pointerdown', { target });
}

function keydown(key: string) {
  const stop = vi.fn();
  win.fire('keydown', { key, stopPropagation: stop });
  return stop;
}

beforeEach(() => vi.stubGlobal('window', win));
function mount(options: Partial<DismissOptions> = {}) {
  const onclose = vi.fn();
  const action = dismiss(node, { onclose, ...options });
  return { onclose, action };
}

describe('dismiss', () => {
  it('closes on a pointer press outside the node', () => {
    const { onclose, action } = mount();
    pointerdown();
    expect(onclose).toHaveBeenCalledOnce();
    action.destroy();
  });

  it('ignores a press inside the node', () => {
    const { onclose, action } = mount();
    pointerdown(inside);
    expect(onclose).not.toHaveBeenCalled();
    action.destroy();
  });

  it('ignores a press on a target it is told to keep', () => {
    const kept = {} as Element;
    const { onclose, action } = mount({ keep: (t) => t === kept });
    pointerdown(kept);
    expect(onclose).not.toHaveBeenCalled();
    pointerdown();
    expect(onclose).toHaveBeenCalledOnce();
    action.destroy();
  });

  it('does nothing while inactive and follows later updates', () => {
    const { onclose, action } = mount({ active: false });
    pointerdown();
    expect(onclose).not.toHaveBeenCalled();
    action.update({ onclose, active: true });
    pointerdown();
    expect(onclose).toHaveBeenCalledOnce();
    action.destroy();
  });

  it('leaves Escape alone unless asked to handle it', () => {
    const { onclose, action } = mount();
    keydown('Escape');
    expect(onclose).not.toHaveBeenCalled();
    action.destroy();
  });

  it('closes on Escape and ignores other keys', () => {
    const { onclose, action } = mount({ escape: true });
    keydown('a');
    expect(onclose).not.toHaveBeenCalled();
    const stop = keydown('Escape');
    expect(onclose).toHaveBeenCalledOnce();
    expect(stop).not.toHaveBeenCalled();
    action.destroy();
  });

  it('stops the Escape from propagating when asked', () => {
    const { onclose, action } = mount({ escape: 'stop' });
    const stop = keydown('Escape');
    expect(onclose).toHaveBeenCalledOnce();
    expect(stop).toHaveBeenCalledOnce();
    action.destroy();
  });

  it('gives focus back to the opener after closing on Escape, not on an outside press', () => {
    const order: string[] = [];
    const restore = vi.fn(() => order.push('restore'));
    const { action } = mount({ escape: true, restore, onclose: () => order.push('close') });
    pointerdown();
    expect(restore).not.toHaveBeenCalled();
    keydown('Escape');
    expect(order).toEqual(['close', 'close', 'restore']);
    action.destroy();
  });

  it('stops listening once destroyed', () => {
    const { onclose, action } = mount({ escape: true });
    action.destroy();
    pointerdown();
    keydown('Escape');
    expect(onclose).not.toHaveBeenCalled();
  });
});
