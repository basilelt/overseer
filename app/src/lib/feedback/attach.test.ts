import { describe, expect, it } from 'vitest';
import { triage } from './attach';

describe('attachment triage', () => {
  it('keeps every refused file of one drop, with the reason for each', () => {
    const { ok, refused } = triage([
      { name: 'notes.txt', size: 10 },
      { name: 'big.png', size: 11 * 1024 * 1024 },
      { name: 'ok.png', size: 1024 },
      { name: 'huge.mp4', size: 40 * 1024 * 1024 },
      { name: 'clip.mov', size: 101 * 1024 * 1024 },
    ]);
    expect(ok.map((f) => f.name)).toEqual(['ok.png', 'huge.mp4']);
    expect(refused).toHaveLength(3);
    expect(refused[0]).toContain('notes.txt');
    expect(refused[1]).toContain('big.png');
    expect(refused[2]).toContain('clip.mov');
  });

  it('refuses a file over the upload limit with its name', () => {
    const { ok, refused } = triage([{ name: 'a.png', size: 51 * 1024 * 1024 }]);
    expect(ok).toEqual([]);
    expect(refused[0]).toContain('a.png');
  });
});
