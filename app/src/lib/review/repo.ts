import type { Line } from '../types';

export function inRepo(line: Pick<Line, 'repo'> | undefined): boolean {
  return !line || line.repo !== 'unknown';
}
