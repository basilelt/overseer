import { describe, expect, it } from 'vitest';
import { flush } from './flush';

describe('flush', () => {
  it('lets every pending promise reaction and queued task run before it resolves', async () => {
    const order: string[] = [];
    setTimeout(() => order.push('timer'), 0);
    void Promise.resolve().then(() => Promise.resolve()).then(() => order.push('chain'));
    queueMicrotask(() => order.push('microtask'));
    await flush();
    expect(order).toContain('chain');
    expect(order).toContain('microtask');
  });
});
