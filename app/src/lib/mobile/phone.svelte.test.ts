import { afterEach, describe, expect, it } from 'vitest';
import { app } from '../state.svelte';
import { isPhone, ui } from './ui.svelte';
import { phoneOrMobile } from './phone.svelte';

afterEach(() => {
  ui.layout = 'desktop';
  app.mobile = false;
});

describe('phone predicates', () => {
  it('isPhone follows the layout only', () => {
    app.mobile = true;
    expect(isPhone()).toBe(false);
    ui.layout = 'phone';
    expect(isPhone()).toBe(true);
  });

  it('phoneOrMobile is true on the native mobile app or the phone layout', () => {
    expect(phoneOrMobile()).toBe(false);
    app.mobile = true;
    expect(phoneOrMobile()).toBe(true);
    app.mobile = false;
    ui.layout = 'phone';
    expect(phoneOrMobile()).toBe(true);
  });
});
