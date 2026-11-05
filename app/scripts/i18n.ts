import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { missing, parseReply, prompt, render, type Strings } from '../src/lib/i18n-sync.ts';
import { en } from '../src/lib/i18n/en.ts';

const dir = new URL('../src/lib/i18n/', import.meta.url);
const args = process.argv.slice(2);
const dry = args.includes('--dry');
const asked = args.filter((a) => !a.startsWith('--'));
const locales = asked.length ? asked : readdirSync(dir).filter((f) => /^[\w-]+\.ts$/.test(f) && f !== 'en.ts').map((f) => f.slice(0, -3));
const BATCH = 150;

async function current(locale: string): Promise<Strings> {
  const file = new URL(`${locale}.ts`, dir);
  return existsSync(file) ? { ...(await import(file.href)).dict } : {};
}

function translate(locale: string, keys: string[]): Strings {
  const reply = execFileSync('claude', ['-p', '--tools', '', '--no-session-persistence', '--output-format', 'text'], {
    input: prompt(locale, en, keys),
    cwd: tmpdir(),
    encoding: 'utf8',
    maxBuffer: 16 << 20,
  });
  return parseReply(reply, en, keys);
}

for (const locale of locales) {
  if (!/^[a-z]{2,3}(-[A-Za-z0-9]+)*$/.test(locale)) throw new Error(`not a BCP 47 tag: ${locale}`);
  const dict = await current(locale);
  const todo = missing(en, dict);
  console.log(`${locale}: ${todo.length} missing of ${Object.keys(en).length}`);
  if (dry || !todo.length) continue;
  for (let i = 0; i < todo.length; i += BATCH) {
    const keys = todo.slice(i, i + BATCH);
    Object.assign(dict, translate(locale, keys));
    writeFileSync(new URL(`${locale}.ts`, dir), render(en, dict));
    console.log(`${locale}: ${Math.min(i + BATCH, todo.length)}/${todo.length}`);
  }
  const left = missing(en, dict).length;
  if (left) console.log(`${locale}: ${left} still missing (rerun to retry)`);
}
