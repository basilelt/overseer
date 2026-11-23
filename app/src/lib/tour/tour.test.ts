import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { tour } from './tour.svelte';
import { saveSettings, settings } from '../settings/settings.svelte';
import { TOUR_VERSION } from './steps';
import { ui } from '../mobile/ui.svelte';

const all = () => true;

describe('tour store', () => {
  beforeEach(() => {
    tour.finish();
    tour.setReady(true);
    saveSettings({ tourSeen: undefined });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    tour.setReady(false);
    tour.setBlocked(false);
  });

  it('does not offer the tour before the shell is ready, then offers it without any timer', () => {
    vi.useFakeTimers();
    tour.setReady(false);
    tour.offer();
    expect(tour.phase).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
    tour.setReady(true);
    tour.offer();
    expect(tour.phase).toBe('welcome');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('offers the welcome screen to new users', () => {
    tour.offer();
    expect(tour.phase).toBe('welcome');
  });

  it('does not offer it again once seen', () => {
    saveSettings({ tourSeen: TOUR_VERSION });
    tour.offer();
    expect(tour.phase).toBeNull();
  });

  it('persists the version when skipped from the welcome screen', () => {
    tour.offer();
    tour.key('close');
    expect(tour.phase).toBeNull();
    expect(settings.tourSeen).toBe(TOUR_VERSION);
  });

  it('navigates with the keyboard and finishes on the last step', async () => {
    await tour.start('full', all);
    const n = tour.steps.length;
    expect(tour.step?.id).toBe('host');
    tour.key('back');
    expect(tour.index).toBe(0);
    tour.key('next');
    tour.key('next');
    expect(tour.index).toBe(2);
    tour.key('back');
    expect(tour.index).toBe(1);
    for (let i = 2; i < n; i++) tour.key('next');
    expect(tour.phase).toBe('tour');
    expect(tour.index).toBe(n - 1);
    expect(settings.tourSeen).toBeUndefined();
    tour.key('next');
    expect(tour.phase).toBeNull();
    expect(settings.tourSeen).toBe(TOUR_VERSION);
  });

  it('starts the tour from the welcome screen with the right arrow', async () => {
    tour.offer();
    tour.key('next');
    await vi.waitFor(() => expect(tour.phase).toBe('tour'));
  });

  it('offers only the new steps to a user who saw an older release', async () => {
    saveSettings({ tourSeen: '0.38' });
    vi.stubGlobal('document', { querySelectorAll: () => [{ getBoundingClientRect: () => ({ width: 10, height: 10 }) }] });
    tour.setReady(true);
    tour.offer();
    await vi.waitFor(() => expect(tour.phase).toBe('tour'));
    expect(tour.mode).toBe('whatsNew');
    expect(tour.steps.length).toBeGreaterThan(0);
    expect(tour.steps.every((s) => s.since === TOUR_VERSION)).toBe(true);
  });

  it('marks the release seen without opening a tour when every new step is desktop only and the user is on a phone', async () => {
    const layout = ui.layout;
    ui.layout = 'phone';
    saveSettings({ tourSeen: '0.38' });
    await tour.start('whatsNew', all);
    expect(tour.phase).toBeNull();
    expect(settings.tourSeen).toBe(TOUR_VERSION);
    ui.layout = layout;
  });

  it('does not offer the new steps while an overlay is open, then offers them once it closes', async () => {
    saveSettings({ tourSeen: '0.35' });
    tour.setBlocked(true);
    tour.offer();
    await Promise.resolve();
    expect(tour.phase).toBeNull();
    tour.setBlocked(false);
    tour.offer();
    await vi.waitFor(() => expect(tour.phase).toBe('tour'));
    expect(tour.mode).toBe('whatsNew');
  });

  it('drops the new steps when an overlay opens while they load, leaving the update unseen', async () => {
    saveSettings({ tourSeen: '0.35' });
    const started = tour.start('whatsNew', all);
    tour.setBlocked(true);
    await started;
    expect(tour.phase).toBeNull();
    expect(settings.tourSeen).toBe('0.35');
    expect(tour.pending()).toBe('whatsNew');
  });

  it('only keeps steps whose target is on screen', async () => {
    await tour.start('full', (n) => n === 'host' || n === 'kanban');
    expect(tour.steps.map((s) => s.id)).toEqual(['host', 'demo', 'kanban', 'palette', 'paletteKey', 'worktreeCleanup', 'merged', 'recordBug', 'quorum', 'taskType', 'quorumAnyAgent', 'taskEdit']);
  });

  it('upgrades an old numeric version to the current release when finished', () => {
    saveSettings({ tourSeen: 1 });
    expect(tour.pending()).toBe('whatsNew');
    tour.finish();
    expect(settings.tourSeen).toBe(TOUR_VERSION);
    expect(tour.pending()).toBeNull();
  });

  it('keeps a newer seen version when finished', () => {
    saveSettings({ tourSeen: '99.0' });
    tour.finish();
    expect(settings.tourSeen).toBe('99.0');
  });

  it('can be replayed after being seen', async () => {
    saveSettings({ tourSeen: TOUR_VERSION });
    await tour.start('full', all);
    expect(tour.phase).toBe('tour');
    expect(tour.mode).toBe('full');
  });
});
