import { describe, expect, it } from 'vitest';
import { basename, bracketed, insertText, MAX_UPLOAD, planDrop, planPaste, shellQuote, uploadedPath } from './uploads';

const png = (name = 'image.png', size = 1000) => ({ name, type: 'image/png', size });

describe('shellQuote', () => {
  it('leaves plain paths alone', () => {
    expect(shellQuote('/tmp/overseer-uploads/shot_1.png')).toBe('/tmp/overseer-uploads/shot_1.png');
    expect(shellQuote('C:/a+b=c,d@e%f:g')).toBe('C:/a+b=c,d@e%f:g');
  });

  it('single-quotes anything else and escapes quotes', () => {
    expect(shellQuote('/Users/me/Screen Shot.png')).toBe(`'/Users/me/Screen Shot.png'`);
    expect(shellQuote(`/tmp/it's.png`)).toBe(`'/tmp/it'\\''s.png'`);
    expect(shellQuote('/tmp/$HOME;rm -rf ~')).toBe(`'/tmp/$HOME;rm -rf ~'`);
    expect(shellQuote('')).toBe(`''`);
  });
});

describe('insertText', () => {
  it('joins quoted paths with a trailing space and never a newline', () => {
    expect(insertText(['/a.png', '/b c.png'])).toBe(`/a.png '/b c.png' `);
    expect(insertText([])).toBe('');
    expect(insertText(['/x\ny'])).toBe(`'/x\ny' `);
    expect(insertText(['/a']).endsWith('\r')).toBe(false);
  });
});

describe('basename', () => {
  it('handles both separators', () => {
    expect(basename('/Users/me/shot.png')).toBe('shot.png');
    expect(basename('C:\\Users\\me\\shot.png')).toBe('shot.png');
    expect(basename('/tmp/dir/')).toBe('dir');
  });
});

describe('planPaste', () => {
  it('lets the terminal paste text when there are no files', () => {
    expect(planPaste({ text: 'hello', files: [] }, false)).toEqual({ kind: 'native' });
    expect(planPaste({ text: '', files: [] }, true)).toEqual({ kind: 'native' });
  });

  it('prefers text when an image only rides along with it', () => {
    expect(planPaste({ text: 'hello', files: [png()] }, true)).toEqual({ kind: 'native' });
    expect(planPaste({ text: ' \n', files: [png()] }, true).kind).toBe('save');
  });

  it('treats a copied image file as a file even though its name is the text', () => {
    expect(planPaste({ text: 'image.png', files: [png()] }, false)).toMatchObject({ kind: 'save', upload: false });
  });

  it('saves a pasted image and uploads it only when remote', () => {
    expect(planPaste({ text: '', files: [png()] }, false)).toEqual({ kind: 'save', upload: false, files: [png()] });
    expect(planPaste({ text: '', files: [png()] }, true)).toEqual({ kind: 'save', upload: true, files: [png()] });
  });

  it('handles copied files even with their name as text', () => {
    const pdf = { name: 'spec.pdf', type: 'application/pdf', size: 10 };
    expect(planPaste({ text: 'notes', files: [pdf] }, true)).toMatchObject({ kind: 'save', upload: true });
  });

  it('refuses files over 50 MiB', () => {
    expect(planPaste({ text: '', files: [png('huge.png', MAX_UPLOAD + 1)] }, true)).toEqual({ kind: 'reject', reason: 'tooLarge', name: 'huge.png' });
    expect(planPaste({ text: '', files: [png('ok.png', MAX_UPLOAD)] }, true).kind).toBe('save');
  });
});

describe('planDrop', () => {
  it('types local paths directly', () => {
    expect(planDrop({ paths: ['/a', '/b'] }, false)).toEqual({ kind: 'type', paths: ['/a', '/b'] });
    expect(planDrop({ paths: ['/dir'], dirs: ['/dir'] }, false)).toEqual({ kind: 'type', paths: ['/dir'] });
  });

  it('uploads remote drops and refuses folders', () => {
    expect(planDrop({ paths: ['/a.png'] }, true)).toEqual({ kind: 'upload', paths: ['/a.png'] });
    expect(planDrop({ paths: ['/a.png', '/src/'], dirs: ['/src/'] }, true)).toEqual({ kind: 'reject', reason: 'folder', name: 'src' });
  });

  it('ignores empty drops', () => {
    expect(planDrop({ paths: [] }, true)).toEqual({ kind: 'none' });
  });
});

describe('uploaded paths', () => {
  it('keeps the short home path unquoted so the shell still expands it', () => {
    expect(shellQuote('~/.overseer/u/a1b2c3.pdf')).toBe('~/.overseer/u/a1b2c3.pdf');
    expect(shellQuote('~/a b.pdf')).toBe(`'~/a b.pdf'`);
    expect(shellQuote('~root/x')).toBe(`'~root/x'`);
  });

  it('gives images their absolute path so Claude Code attaches them, other files the short one', () => {
    const r = (path: string) => ({ path: `/home/u/.overseer/u/${path}`, short: `~/.overseer/u/${path}` });
    expect(uploadedPath(r('a1b2c3.png'))).toBe('/home/u/.overseer/u/a1b2c3.png');
    expect(uploadedPath(r('a1b2c3.JPEG'))).toBe('/home/u/.overseer/u/a1b2c3.JPEG');
    expect(uploadedPath(r('a1b2c3.pdf'))).toBe('~/.overseer/u/a1b2c3.pdf');
    expect(uploadedPath({ path: '/srv/old.txt' })).toBe('/srv/old.txt');
  });

  it('wraps the text in a bracketed paste only when the program asked for it', () => {
    expect(bracketed('/a.png ', true)).toBe('\x1b[200~/a.png \x1b[201~');
    expect(bracketed('/a.png ', false)).toBe('/a.png ');
    expect(bracketed('', true)).toBe('');
    expect(bracketed('x\x1b[201~rm -rf ~\r', true)).toBe('\x1b[200~xrm -rf ~\r\x1b[201~');
  });
});
