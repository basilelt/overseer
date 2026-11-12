import { describe, expect, it } from 'vitest';
import { hostColor, hostName } from './hosts';

describe('hostColor', () => {
  it('keeps local green and gives every SSH host a stable lane colour', () => {
    expect(hostColor('local')).toBe('var(--st-input)');
    expect(hostColor('ssh:prod')).toBe(hostColor('ssh:prod'));
    expect(hostColor('ssh:prod')).toMatch(/^var\(--lane-[1-4]\)$/);
    expect(['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'].map((h) => hostColor(`ssh:${h}`))).not.toContain('var(--lane-5)');
    const used = new Set(['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'].map((h) => hostColor(`ssh:${h}`)));
    expect(used.size).toBeGreaterThan(1);
  });
});

describe('hostName', () => {
  it('names local, then the label, then the host', () => {
    expect(hostName({ kind: 'local', label: '' })).toBe('Local');
    expect(hostName({ kind: 'ssh', host: 'prod', label: 'Prod' })).toBe('Prod');
    expect(hostName({ kind: 'ssh', host: 'prod', label: '' })).toBe('prod');
  });
});
