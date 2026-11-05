import { Lru } from './lru';
import type { Related } from './review/types';
import type { Diff } from './types';

export const diffs = new Lru<string, Diff>(12, 2_000_000, (d) => 64 + d.diff.length + d.stat.length + d.status.length);
export const relateds = new Lru<string, Related>(12);
