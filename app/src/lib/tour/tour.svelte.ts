import { saveSettings, settings } from '../settings/settings.svelte';
import { compareVersions, modeFor, seenVersion, STEPS, stepsFor, TOUR_VERSION, visibleSteps, type KeyAction, type Mode, type Step } from './steps';

export type Phase = null | 'welcome' | 'tour';

export function hasTarget(name: string): boolean {
  if (typeof document === 'undefined') return false;
  for (const el of document.querySelectorAll(`[data-tour="${name}"]`)) {
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) return true;
  }
  return false;
}

class Tour {
  phase = $state<Phase>(null);
  mode = $state<Mode>('full');
  steps = $state.raw<Step[]>([]);
  index = $state(0);
  ready = $state(false);
  step = $derived<Step | null>(this.phase === 'tour' ? (this.steps[this.index] ?? null) : null);

  pending(): Mode | null {
    return modeFor(settings.tourSeen);
  }

  setReady(ready: boolean) {
    this.ready = ready;
  }

  offer() {
    if (this.phase || !this.ready) return;
    const mode = this.pending();
    if (!mode) return;
    if (mode === 'full') {
      this.mode = 'full';
      this.phase = 'welcome';
    } else this.start('whatsNew');
  }

  start(mode: Mode = 'full', has: (name: string) => boolean = hasTarget) {
    const steps = visibleSteps(stepsFor(mode, settings.tourSeen, STEPS), has);
    if (!steps.length) return this.finish();
    this.mode = mode;
    this.steps = steps;
    this.index = 0;
    this.phase = 'tour';
  }

  next() {
    if (this.index >= this.steps.length - 1) return this.finish();
    this.index++;
  }

  back() {
    if (this.index > 0) this.index--;
  }

  key(action: KeyAction) {
    if (action === 'close') return this.finish();
    if (this.phase === 'welcome') return action === 'next' ? this.start() : undefined;
    if (action === 'next') this.next();
    else this.back();
  }

  finish() {
    this.phase = null;
    this.steps = [];
    this.index = 0;
    if (compareVersions(seenVersion(settings.tourSeen) ?? '0', TOUR_VERSION) < 0) saveSettings({ tourSeen: TOUR_VERSION });
  }
}

export const tour = new Tour();
