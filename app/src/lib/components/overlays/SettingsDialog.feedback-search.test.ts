import { describe, expect, it } from 'vitest';
import src from './SettingsDialog.svelte?raw';

const feedbackSection = () => src.slice(src.indexOf("{#if vis('feedback')}"), src.indexOf('</Modal>'));

describe('settings feedback section search', () => {
  it.each(['feedback', 'feedbackAgent', 'privacy'])('hides the %s row when another row of the section matches', (id) => {
    expect(feedbackSection()).toContain(`row('feedback', '${id}')`);
  });
});
