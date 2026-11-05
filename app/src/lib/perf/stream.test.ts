import { describe, expect, it } from 'vitest';
import { STREAM_LINES, streamBlock } from './stream';

describe('perf stream', () => {
  it('emits a fixed number of CRLF lines per block', () => {
    const b = streamBlock(3);
    expect(b.split('\r\n').length - 1).toBe(STREAM_LINES);
    expect(b).toContain('0000030');
    expect(streamBlock(3)).toBe(b);
  });
});
