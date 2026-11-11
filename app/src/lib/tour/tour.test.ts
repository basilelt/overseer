import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { tour } from './tour.svelte';
import { saveSettings, settings } from '../settings.svelte';
import { TOUR_VERSION } from './steps';

const all = () => true;

describe('tour store', () => {
  beforeEach(() => {
    tour.finish();
    tour.setReady(true);
    saveSettings({ tourSeen: undefined });
  });

  afterEach(() => {
    vi.useRealTimers();
    tour.setReady(false);
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

  it('navigates with the keyboard and finishes on the last step', () => {
    tour.start('full', all);
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

  it('starts the tour from the welcome screen with the right arrow', () => {
    tour.offer();
    tour.key('next');
    expect(tour.phase).toBe('tour');
  });

  it('only keeps steps whose target is on screen', () => {
    tour.start('full', (n) => n === 'host' || n === 'kanban');
    expect(tour.steps.map((s) => s.id)).toEqual(['host', 'demo', 'kanban', 'palette', 'paletteKey', 'worktreeCleanup', 'merged', 'recordBug', 'quorum']);
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

  it('can be replayed after being seen', () => {
    saveSettings({ tourSeen: TOUR_VERSION });
    tour.start('full', all);
    expect(tour.phase).toBe('tour');
    expect(tour.mode).toBe('full');
  });
});
