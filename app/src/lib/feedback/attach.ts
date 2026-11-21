import { attachProblem } from './feedback';
import { ft } from './feedback.svelte';
import { MAX_UPLOAD } from '../uploads';

export function triage<T extends { name: string; size: number }>(items: T[]): { ok: T[]; refused: string[] } {
  const ok: T[] = [];
  const refused: string[] = [];
  for (const it of items) {
    const p = it.size > MAX_UPLOAD ? 'big' : attachProblem(it.name, it.size);
    if (!p) ok.push(it);
    else if (p === 'big') refused.push(ft('failed', { error: `${it.name}: > 50 MB` }));
    else refused.push(ft(p === 'type' ? 'attachType' : p === 'image' ? 'attachImageBig' : 'attachVideoBig', { name: it.name }));
  }
  return { ok, refused };
}
