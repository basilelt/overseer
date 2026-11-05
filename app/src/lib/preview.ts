import { app } from './state.svelte';

export function openFile(path: string, term?: string, line?: string): void {
  app.open('file', { path, term, line });
}
