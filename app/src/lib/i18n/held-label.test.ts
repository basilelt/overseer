import { describe, expect, it } from 'vitest';
import { en } from './en';
import { dict as de } from './de';
import { dict as es } from './es';
import { dict as fr } from './fr';

const locales = { en, fr, de, es } as const;
const STATES = ['running', 'attention', 'open', 'paused', 'closed'] as const;
const SAME_STATE = {
  closed: ['group.stopped', 'wsstate.stopped', 'session.state.closed'],
  paused: ['group.paused', 'wsstate.paused', 'session.state.paused'],
  open: ['group.ready', 'wsstate.ready', 'session.state.open'],
} as const;

describe.each(Object.entries(locales))('session state labels in %s', (_, dict) => {
  it('gives each of the five states its own label', () => {
    const labels = STATES.map((s) => dict[`session.state.${s}`] ?? en[`session.state.${s}`]);
    expect(new Set(labels).size).toBe(STATES.length);
  });

  it.each(Object.entries(SAME_STATE))('names a %s session the same way in the group, workspace and state labels', (_, keys) => {
    expect(new Set(keys.map((k) => dict[k] ?? en[k])).size).toBe(1);
  });

  it('tells a stopped session from a paused one in the chat banner', () => {
    expect(dict['chat.held'] ?? en['chat.held']).not.toBe(dict['chat.paused'] ?? en['chat.paused']);
  });
});
