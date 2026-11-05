import { describe, expect, it } from 'vitest';

const styles = import.meta.glob<string>(['../**/*.svelte', '../**/*.css'], { query: '?raw', import: 'default', eager: true });

describe('user-select in the WebKit desktop webviews', () => {
  it('pairs every user-select rule with -webkit-user-select, which WebKit needs', () => {
    const missing: string[] = [];
    for (const [file, text] of Object.entries(styles)) {
      const lines = text.split('\n');
      lines.forEach((line, i) => {
        const m = /^\s*user-select:\s*([a-z]+);/.exec(line);
        if (!m) return;
        const near = lines.slice(Math.max(0, i - 2), i + 3).join('\n');
        if (!near.includes(`-webkit-user-select: ${m[1]};`)) missing.push(`${file}:${i + 1}`);
      });
    }
    expect(Object.keys(styles).length).toBeGreaterThan(50);
    expect(missing).toEqual([]);
  });
});
