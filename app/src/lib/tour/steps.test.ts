import { describe, expect, it } from 'vitest';
import { compareVersions, keyAction, modeFor, seenVersion, placeCard, STEPS, stepsFor, targetOf, TOUR_VERSION, union, visibleSteps, type Step } from './steps';

const steps: Step[] = [
  { id: 'host', targets: ['host'], since: '0.15' },
  { id: 'notify', targets: ['hooks', 'settings'], since: '0.15' },
  { id: 'palette', targets: ['palette'], since: '0.15', center: true },
  { id: 'review', targets: ['review'], since: '0.17' },
];

describe('tour steps', () => {
  it('skips steps whose targets are missing', () => {
    const has = (n: string) => n === 'host';
    expect(visibleSteps(steps, has).map((s) => s.id)).toEqual(['host', 'palette']);
  });

  it('keeps every step when all targets are present', () => {
    expect(visibleSteps(STEPS, () => true)).toHaveLength(STEPS.length);
  });

  it('falls back to the next target in order', () => {
    expect(targetOf(steps[1], (n) => n === 'settings')).toBe('settings');
    expect(targetOf(steps[1], () => true)).toBe('hooks');
    expect(targetOf(steps[1], () => false)).toBeNull();
  });

  it('has unique ids and a minor or major release version on every step', () => {
    expect(new Set(STEPS.map((s) => s.id)).size).toBe(STEPS.length);
    expect(STEPS.every((s) => /^\d+\.\d+$/.test(s.since) && compareVersions(s.since, TOUR_VERSION) <= 0)).toBe(true);
  });

  it('takes the newest step version as the tour version', () => {
    expect(STEPS.some((s) => s.since === TOUR_VERSION)).toBe(true);
  });
});

describe('tour versioning', () => {
  const releases: Step[] = [
    ...steps,
    { id: 'chips', targets: ['chips'], since: '0.23' },
    { id: 'kanban', targets: ['kanban'], since: '0.24' },
    { id: 'organize', targets: ['organize'], since: '0.26' },
    { id: 'feedback', targets: ['feedback'], since: '0.30' },
  ];

  it('compares versions by number, not text', () => {
    expect(compareVersions('0.9', '0.10')).toBeLessThan(0);
    expect(compareVersions('1.0', '0.26')).toBeGreaterThan(0);
    expect(compareVersions('0.17', '0.17.0')).toBe(0);
    expect(compareVersions('1', '1.0')).toBe(0);
  });

  it('shows the full tour to new users', () => {
    expect(modeFor(undefined, '0.17')).toBe('full');
    expect(modeFor(0, '0.17')).toBe('full');
  });

  it('shows only the steps of the release after the one seen', () => {
    expect(modeFor('0.15', '0.17')).toBe('whatsNew');
    expect(stepsFor('whatsNew', '0.15', steps).map((s) => s.id)).toEqual(['review']);
    expect(stepsFor('full', '0.15', steps)).toHaveLength(steps.length);
  });

  it('shows every release missed since the one seen, oldest first', () => {
    expect(modeFor('0.22', '0.26')).toBe('whatsNew');
    expect(stepsFor('whatsNew', '0.22', releases).map((s) => s.id)).toEqual(['chips', 'kanban', 'organize', 'feedback']);
    expect(stepsFor('whatsNew', '0.24', releases).map((s) => s.id)).toEqual(['organize', 'feedback']);
  });

  it('orders missed steps by release, whatever their order in the list', () => {
    expect(stepsFor('whatsNew', '0.15', [...releases].reverse()).map((s) => s.since)).toEqual(['0.17', '0.23', '0.24', '0.26', '0.30']);
  });

  it('maps the old numeric tour versions to their releases', () => {
    expect(seenVersion(1)).toBe('0.15');
    expect(seenVersion(2)).toBe('0.17');
    expect(seenVersion(7)).toBe('0.17');
    expect(seenVersion(0)).toBeUndefined();
    expect(seenVersion(undefined)).toBeUndefined();
    expect(stepsFor('whatsNew', 1, releases).map((s) => s.id)).toEqual(['review', 'chips', 'kanban', 'organize', 'feedback']);
    expect(stepsFor('whatsNew', 2, releases).map((s) => s.id)).toEqual(['chips', 'kanban', 'organize', 'feedback']);
  });

  it('shows the organize step to users who saw the 0.15 tour', () => {
    expect(modeFor(1)).toBe('whatsNew');
    expect(stepsFor('whatsNew', 1).map((s) => s.id)).toEqual(['organize', 'settings', 'paletteKey', 'demo', 'gpuTerminal', 'worktreeCleanup', 'switchboard', 'clickCursor', 'palettes']);
    expect(stepsFor('full', undefined).map((s) => s.id)).toContain('organize');
    expect(targetOf(STEPS.find((s) => s.id === 'organize')!, (n) => n === 'workspaces')).toBe('workspaces');
  });

  it('shows the 0.25 to 0.29 steps to users who saw 0.24', () => {
    expect(modeFor('0.24')).toBe('whatsNew');
    expect(stepsFor('whatsNew', '0.24').map((s) => s.id)).toEqual(['settings', 'paletteKey', 'demo', 'gpuTerminal', 'worktreeCleanup', 'switchboard', 'clickCursor', 'palettes']);
    expect(stepsFor('full', undefined).map((s) => s.id)).toContain('settings');
    expect(targetOf(STEPS.find((s) => s.id === 'settings')!, (n) => n === 'settings')).toBe('settings');
  });

  it('shows the GPU rendering, worktree cleanup and Switchboard steps to users who saw 0.27', () => {
    expect(modeFor('0.27')).toBe('whatsNew');
    expect(stepsFor('whatsNew', '0.27').map((s) => s.id)).toEqual(['gpuTerminal', 'worktreeCleanup', 'switchboard', 'clickCursor', 'palettes']);
  });

  it('points the demo step at the Demo mode pill in the demo, else at the host chip', () => {
    const demo = STEPS.find((s) => s.id === 'demo')!;
    expect(targetOf(demo, (n) => n === 'demo' || n === 'host')).toBe('demo');
    expect(targetOf(demo, (n) => n === 'host')).toBe('host');
  });

  it('points the settings step at the More tab on a phone', () => {
    expect(targetOf(STEPS.find((s) => s.id === 'settings')!, (n) => n === 'palette')).toBe('palette');
  });

  it('shows the steps added after 0.17 to users who saw the 0.17 tour', () => {
    expect(modeFor('0.17')).toBe('whatsNew');
    expect(stepsFor('whatsNew', '0.17').map((s) => s.id)).toEqual(['settings', 'paletteKey', 'demo', 'gpuTerminal', 'worktreeCleanup', 'switchboard', 'clickCursor', 'palettes']);
    expect(STEPS.find((s) => s.id === 'paletteKey')?.since).toBe('0.25');
  });

  it('shows the 0.27, 0.28 and 0.29 steps to users who saw 0.26', () => {
    expect(modeFor('0.26')).toBe('whatsNew');
    expect(stepsFor('whatsNew', '0.26').map((s) => s.id)).toEqual(['demo', 'gpuTerminal', 'worktreeCleanup', 'switchboard', 'clickCursor', 'palettes']);
    expect(STEPS.find((s) => s.id === 'gpuTerminal')?.since).toBe('0.28');
  });

  it('shows the worktree cleanup and Switchboard steps to users who saw 0.28', () => {
    expect(TOUR_VERSION).toBe('0.31');
    expect(modeFor('0.28')).toBe('whatsNew');
    expect(stepsFor('whatsNew', '0.28').map((s) => s.id)).toEqual(['worktreeCleanup', 'switchboard', 'clickCursor', 'palettes']);
    expect(targetOf(STEPS.find((s) => s.id === 'switchboard')!, (n) => n === 'switchboard')).toBe('switchboard');
    expect(modeFor('0.31')).toBeNull();
    expect(targetOf(STEPS.find((s) => s.id === 'worktreeCleanup')!, (n) => n === 'palette')).toBe('palette');
  });

  it('shows the palettes step to users who saw 0.29 or 0.30', () => {
    expect(stepsFor('whatsNew', '0.29').map((s) => s.id)).toEqual(['clickCursor', 'palettes']);
    expect(modeFor('0.30')).toBe('whatsNew');
    expect(stepsFor('whatsNew', '0.30').map((s) => s.id)).toEqual(['palettes']);
    const step = STEPS.find((s) => s.id === 'palettes')!;
    expect(step.since).toBe('0.31');
    expect(targetOf(step, (n) => n === 'settings')).toBe('settings');
    expect(targetOf(step, (n) => n === 'palette')).toBe('palette');
  });

  it('shows the click-to-move-cursor step to users who saw 0.29', () => {
    expect(modeFor('0.29')).toBe('whatsNew');
    expect(stepsFor('whatsNew', '0.29').map((s) => s.id)).toEqual(['clickCursor', 'palettes']);
    expect(targetOf(STEPS.find((s) => s.id === 'clickCursor')!, (n) => n === 'tabs')).toBe('tabs');
  });

  it('points the GPU rendering step at settings, or the More tab on a phone', () => {
    const step = STEPS.find((s) => s.id === 'gpuTerminal')!;
    expect(targetOf(step, (n) => n === 'settings')).toBe('settings');
    expect(targetOf(step, (n) => n === 'palette')).toBe('palette');
  });

  it('points the Switchboard step at its Settings button, else Settings or the palette', () => {
    const step = STEPS.find((s) => s.id === 'switchboard')!;
    expect(step.since).toBe('0.29');
    expect(targetOf(step, (n) => n === 'switchboard' || n === 'settings')).toBe('switchboard');
    expect(targetOf(step, (n) => n === 'settings')).toBe('settings');
    expect(targetOf(step, (n) => n === 'palette')).toBe('palette');
  });

  it('shows nothing once the current release was seen', () => {
    expect(modeFor('0.25', '0.25')).toBeNull();
    expect(modeFor('0.26', '0.25')).toBeNull();
    expect(modeFor('0.31')).toBeNull();
    expect(modeFor(2, '0.17')).toBeNull();
  });
});

describe('tour keys', () => {
  it('maps arrows and escape', () => {
    expect(keyAction('ArrowRight')).toBe('next');
    expect(keyAction('ArrowLeft')).toBe('back');
    expect(keyAction('Escape')).toBe('close');
    expect(keyAction('Enter')).toBeNull();
    expect(keyAction('a')).toBeNull();
  });
});

describe('tour geometry', () => {
  it('unions the visible rects', () => {
    expect(
      union([
        { top: 10, left: 100, width: 20, height: 10 },
        { top: 8, left: 130, width: 30, height: 14 },
        { top: 0, left: 0, width: 0, height: 0 },
      ]),
    ).toEqual({ top: 8, left: 100, width: 60, height: 14 });
    expect(union([])).toBeNull();
  });

  const view = { width: 1200, height: 800 };
  const card = { width: 320, height: 150 };

  it('places the card below the target when it fits', () => {
    expect(placeCard({ top: 10, left: 1100, width: 80, height: 24 }, card, view)).toEqual({ top: 46, left: 868 });
  });

  it('places the card above a target near the bottom', () => {
    expect(placeCard({ top: 700, left: 40, width: 80, height: 40 }, card, view)).toEqual({ top: 538, left: 40 });
  });

  it('places the card beside a tall target', () => {
    expect(placeCard({ top: 40, left: 0, width: 260, height: 760 }, card, view)).toEqual({ top: 40, left: 272 });
  });
});
