import { describe, expect, it } from 'vitest';
import { CONFIRM_WINDOW_MS, CONNECT_CLOSE_DELAY_MS, COPIED_FLASH_MS } from './timing';

describe('timing', () => {
  it('keeps the delays the surfaces shipped with', () => {
    expect([COPIED_FLASH_MS, CONFIRM_WINDOW_MS, CONNECT_CLOSE_DELAY_MS]).toEqual([1200, 3000, 600]);
  });
});
