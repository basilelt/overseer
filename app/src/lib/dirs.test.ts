import { describe, expect, it } from 'vitest';
import { crumbs, shortPath, tildePath } from './dirs';

describe('tildePath', () => {
  it('writes the home directory as ~', () => {
    expect(tildePath('/home/me', '/home/me')).toBe('~');
    expect(tildePath('/home/me/src/x/', '/home/me')).toBe('~/src/x');
    expect(tildePath('/home/meow/x', '/home/me')).toBe('/home/meow/x');
    expect(tildePath('/opt/x', '/')).toBe('/opt/x');
  });
});

describe('crumbs', () => {
  it('starts at ~ inside the home directory', () => {
    expect(crumbs('/home/me/src/app', '/home/me')).toEqual([
      { label: '~', path: '/home/me' },
      { label: 'src', path: '/home/me/src' },
      { label: 'app', path: '/home/me/src/app' },
    ]);
    expect(crumbs('/home/me/', '/home/me')).toEqual([{ label: '~', path: '/home/me' }]);
  });

  it('starts at / elsewhere', () => {
    expect(crumbs('/', '/home/me')).toEqual([{ label: '/', path: '/' }]);
    expect(crumbs('/home/meow/x', '/home/me')).toEqual([
      { label: '/', path: '/' },
      { label: 'home', path: '/home' },
      { label: 'meow', path: '/home/meow' },
      { label: 'x', path: '/home/meow/x' },
    ]);
  });
});

describe('shortPath', () => {
  it('replaces a Linux or macOS home prefix with ~', () => {
    expect(shortPath('/home/alice/src/app')).toBe('~/src/app');
    expect(shortPath('/Users/bob')).toBe('~');
  });

  it('leaves paths outside a home folder alone', () => {
    expect(shortPath('/opt/homebrew')).toBe('/opt/homebrew');
    expect(shortPath('/home2/alice')).toBe('/home2/alice');
  });
});
