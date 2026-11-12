import { describe, expect, it } from 'vitest';
import { basename, folderName } from './paths';

describe('basename', () => {
  it('handles both separators', () => {
    expect(basename('/Users/me/shot.png')).toBe('shot.png');
    expect(basename('C:\\Users\\me\\shot.png')).toBe('shot.png');
    expect(basename('/tmp/dir/')).toBe('dir');
  });

  it('keeps the whole input when there is no segment', () => {
    expect(basename('/')).toBe('/');
    expect(basename('')).toBe('');
  });
});

describe('folderName', () => {
  it('takes the last segment and ignores trailing slashes', () => {
    expect(folderName('/home/me/src/app/')).toBe('app');
    expect(folderName(' ~/src/my-project ')).toBe('my-project');
    expect(folderName('~')).toBe('~');
  });

  it('is empty for the root and does not split backslashes', () => {
    expect(folderName('/')).toBe('');
    expect(folderName('C:\\src\\web')).toBe('C:\\src\\web');
  });
});
