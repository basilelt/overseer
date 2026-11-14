import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import Composer from './Composer.svelte';
import composerSrc from './Composer.svelte?raw';

describe('Composer', () => {
  it('renders a message box the user can type into', () => {
    const h = render(Composer, { props: { lineId: 'l1', agent: 'claude', local: undefined, phone: false } }).body;
    expect(h).toContain('<textarea');
  });

  it('source guard: the Send button does not take focus from the box, so the caret stays after a send', () => {
    const send = composerSrc.match(/<button class="send"[\s\S]*?<\/button>/)?.[0] ?? '';
    expect(send).toContain('onpointerdown={(e) => e.preventDefault()}');
  });
});
