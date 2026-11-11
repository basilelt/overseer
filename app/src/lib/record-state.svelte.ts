export type Phase = 'idle' | 'setup' | 'starting' | 'recording';

export const rec = $state<{ phase: Phase }>({ phase: 'idle' });

export const tap: { note: ((kind: 'toast', text: string) => void) | null } = { note: null };

export function setupRecording() {
  if (rec.phase === 'idle') rec.phase = 'setup';
}
