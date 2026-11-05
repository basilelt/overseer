import { describe, expect, it } from 'vitest';
import { baseName, crumbs, tildePath } from './dirs';

describe('baseName', () => {
  it('takes the last segment and ignores trailing slashes', () => {
    expect(baseName('/home/me/src/app/')).toBe('app');
    expect(baseName(' ~/src/my-project ')).toBe('my-project');
    expect(baseName('~')).toBe('~');
    expect(baseName('/')).toBe('');
  });
});

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
