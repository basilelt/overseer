import { describe, expect, it } from 'vitest';
import { missing, parseReply, prompt, render } from './i18n-sync';

const en = { 'a.title': 'Title', 'a.count': '{n} files in {dir}', 'a.quote': "It's here" };

describe('missing', () => {
  it('lists English keys the locale lacks, in English order', () => {
    expect(missing(en, { 'a.count': '{n} fichiers dans {dir}' })).toEqual(['a.title', 'a.quote']);
  });

  it('is empty when every key is translated', () => {
    expect(missing(en, { 'a.title': 'T', 'a.count': 'C', 'a.quote': 'Q' })).toEqual([]);
  });
});

describe('render', () => {
  it('writes a typed locale module in English key order and drops stale keys', () => {
    const out = render(en, { 'a.quote': "C'est ici", 'a.title': 'Titre', gone: 'x' });
    expect(out).toBe(`import type { Dict } from '../i18n.svelte';\n\nexport const dict: Partial<Dict> = {\n  'a.title': 'Titre',\n  'a.quote': "C'est ici",\n};\n`);
  });

  it('escapes backslashes, newlines and double quotes', () => {
    const out = render({ k: '' }, { k: 'a\\b\n"c"' });
    expect(out).toContain(`  'k': 'a\\\\b\\n"c"',\n`);
  });
});

describe('prompt', () => {
  it('names the target language and sends only the requested strings', () => {
    const p = prompt('de', en, ['a.count']);
    expect(p).toContain('"de"');
    expect(p).toContain('"a.count": "{n} files in {dir}"');
    expect(p).not.toContain('a.title');
  });
});

describe('parseReply', () => {
  it('keeps translations that preserve every placeholder', () => {
    const reply = 'Sure:\n```json\n{"a.count": "{dir} enthält {n} Dateien", "a.title": "Titel"}\n```';
    expect(parseReply(reply, en, ['a.count', 'a.title'])).toEqual({ 'a.count': '{dir} enthält {n} Dateien', 'a.title': 'Titel' });
  });

  it('drops translations that lose or rename a placeholder', () => {
    expect(parseReply('{"a.count": "{n} Dateien", "a.title": "Titel {x}"}', en, ['a.count', 'a.title'])).toEqual({});
  });

  it('ignores keys it did not ask for, empty strings and non-strings', () => {
    expect(parseReply('{"a.title": "  ", "a.quote": 3, "other": "x"}', en, ['a.title', 'a.quote'])).toEqual({});
  });

  it('returns nothing for a reply without valid JSON', () => {
    expect(parseReply('I cannot help', en, ['a.title'])).toEqual({});
    expect(parseReply('{not json}', en, ['a.title'])).toEqual({});
    expect(parseReply('null}', en, ['a.title'])).toEqual({});
  });
});
