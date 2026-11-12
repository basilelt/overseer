import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { sessionState } from '../../session/session-state';
import type { LocalLine } from '../../types';
import SessionBar from './SessionBar.svelte';

const view = (state: LocalLine['line']['state'], local: LocalLine['local']) =>
  sessionState({ line: { id: 'l1', agent: 'claude', state, lastActivityAt: '' }, local } as LocalLine);

const html = (v: ReturnType<typeof view>, extra: Record<string, unknown> = {}) =>
  render(SessionBar, { props: { view: v, acting: null, confirmClose: false, onact: () => {}, ...extra } }).body.split(/<!--[\s\S]*?--!?>/).join('');

const button = (h: string, action: string) => h.match(new RegExp(`<button[^>]*data-action="${action}"[^>]*>`))?.[0];

describe('SessionBar', () => {
  it('enables interrupt and pause on a running session and disables resume with the reason as tooltip', () => {
    // Given a running session
    // When the bar renders
    // Then interrupt and pause are enabled and resume is disabled with its reason
    const h = html(view('active', { pid: 1, busy: true }));
    expect(button(h, 'interrupt')).not.toContain('disabled');
    expect(button(h, 'pause')).not.toContain('disabled');
    expect(button(h, 'resume')).toContain('disabled');
    expect(button(h, 'resume')).toContain('The session is already running');
  });

  it('shows only resume and close as live actions on a paused session', () => {
    // Given a session paused on purpose
    // When the bar renders
    // Then interrupt and pause say why they are off, resume and close are on
    const h = html(view('held', { heldReason: 'user' }));
    expect(button(h, 'interrupt')).toContain('No agent process is running');
    expect(button(h, 'pause')).toContain('disabled');
    expect(button(h, 'resume')).not.toContain('disabled');
    expect(button(h, 'close')).not.toContain('disabled');
  });

  it('disables resume with its reason while a resumed agent is still starting in its terminal', () => {
    // Given a stopped session whose resume is waiting on the agent's first prompt
    // When the bar renders
    // Then resume is off and says the agent is starting, close stays on
    const v = sessionState({ line: { id: 'l1', agent: 'claude', state: 'held', lastActivityAt: '' }, local: { heldReason: 'lost' } } as LocalLine, 't1');
    const h = html(v);
    expect(button(h, 'resume')).toContain('disabled');
    expect(button(h, 'resume')).toContain('The agent is starting in its terminal');
    expect(button(h, 'close')).not.toContain('disabled');
  });

  it('swaps pause for cancel pause only while a pause is queued', () => {
    // Given a busy session with a queued pause, then one without
    // When the bar renders
    // Then cancel pause replaces pause only for the first
    const queued = html(view('active', { pid: 1, busy: true, holdPending: true }));
    expect(button(queued, 'cancelPause')).not.toContain('disabled');
    expect(button(queued, 'pause')).toBeUndefined();
    const plain = html(view('active', { pid: 1, busy: true }));
    expect(button(plain, 'pause')).toBeDefined();
    expect(button(plain, 'cancelPause')).toBeUndefined();
  });

  it('disables everything on a closed session and gives each button an accessible label', () => {
    // Given a closed session
    // When the bar renders
    // Then every action is disabled and labelled
    const h = html(view('hung-up', {}));
    for (const a of ['interrupt', 'pause', 'resume', 'close']) {
      expect(button(h, a)).toContain('disabled');
      expect(button(h, a)).toContain('aria-label=');
    }
  });

  it('asks to confirm before closing', () => {
    // Given a close awaiting confirmation
    // When the bar renders
    // Then the close button shows the confirmation text
    expect(html(view('active', { pid: 1 }), { confirmClose: true })).toContain('Close?');
  });
});
