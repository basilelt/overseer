import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import { app } from '../../state.svelte';
import { settings } from '../../settings/settings.svelte';
import { feedback } from '../../feedback/feedback.svelte';
import type { AgentInfo } from '../../types';
import Sheet from './FeedbackSheet.svelte';

const agent = (id: string, name: string, installed = true) => ({ id, name, installed, efforts: [] }) as unknown as AgentInfo;

function html(): string {
  let h = render(Sheet).body;
  for (let prev = ''; prev !== h; ) {
    prev = h;
    h = h.replace(/<!--[\s\S]*?--!?>/g, '');
  }
  return h;
}

function button(h: string, label: string): string {
  return h.match(new RegExp(`<button[^>]*>${label}</button>`))?.[0] ?? '';
}

describe('feedback sheet send buttons', () => {
  beforeEach(() => {
    app.connected = true;
    app.agents = [agent('claude', 'Claude Code'), agent('codex', 'Codex')];
    Object.assign(settings, { defaultAgent: 'claude', feedbackAgent: undefined });
    feedback.show({ kind: 'bug' });
  });

  afterEach(() => {
    feedback.close();
    app.connected = false;
    app.agents = [];
    vi.restoreAllMocks();
  });

  it('offers Send with… next to the primary button when two CLIs are installed', () => {
    const h = html();
    expect(h).toContain('data-tour="feedback-cli"');
    expect(h).toContain('Send with Claude Code');
  });

  it('hides Send with… when only one CLI is installed', () => {
    app.agents = [agent('claude', 'Claude Code'), agent('codex', 'Codex', false)];
    expect(html()).not.toContain('data-tour="feedback-cli"');
  });

  it('uses the feedback CLI for the primary button', () => {
    settings.feedbackAgent = 'codex';
    expect(html()).toContain('Send with Codex');
  });

  it('disables Send with… while the diagnostics are still loading, like the primary button', () => {
    const h = html();
    expect(button(h, 'Send with…')).toMatch(/\bdisabled\b/);
    expect(button(h, 'Send with Claude Code')).toMatch(/\bdisabled\b/);
  });
});

describe('feedback sheet inline errors', () => {
  beforeEach(() => {
    app.connected = true;
    feedback.show({ kind: 'bug' });
  });

  afterEach(() => {
    feedback.close();
    app.connected = false;
    vi.restoreAllMocks();
  });

  it('shows no alert on a fresh sheet', () => {
    expect(html()).not.toContain('role="alert"');
  });

  it('shows the description error next to the description field and links it', () => {
    feedback.descError = 'Describe the bug or idea first';
    const h = html();
    expect(h).toMatch(/<span[^>]*id="fb-desc-error"[^>]*role="alert"[^>]*>Describe the bug or idea first<\/span>/);
    expect(h).toMatch(/<textarea[^>]*aria-invalid="true"[^>]*aria-describedby="fb-desc-error"/);
  });

  it('lists every refused file in the attachments field', () => {
    feedback.refused = ['notes.txt: only some types.', 'big.png: images can be 10 MB at most.'];
    const h = html();
    expect(h).toContain('<li>notes.txt: only some types.</li>');
    expect(h).toContain('<li>big.png: images can be 10 MB at most.</li>');
  });
});
