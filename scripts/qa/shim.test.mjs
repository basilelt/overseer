import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const src = readFileSync(join(here, 'shim.js'), 'utf8');
const table = readFileSync(join(here, '../../app/src/lib/daemon/tauri-commands.ts'), 'utf8');

let failed = 0;
const test = async (name, fn) => {
  try {
    await fn();
    console.log('ok   shim ' + name);
  } catch (e) {
    failed = 1;
    console.log('FAIL shim ' + name + ': ' + (e?.message ?? e));
  }
};

const sandbox = { console: { error() {} }, Uint8Array, ArrayBuffer, TextEncoder, TextDecoder, setTimeout, clearInterval, setInterval, URLSearchParams, location: { search: '' } };
sandbox.window = sandbox;
vm.runInNewContext(src, sandbox);
const invoke = sandbox.__TAURI_INTERNALS__.invoke;
const paste = (name, bytes = new Uint8Array(3)) => invoke('save_paste', bytes, { headers: { 'x-name': encodeURIComponent(name) } }).then((p) => p.replace(/^\/tmp\/overseer-paste\/\d+-/, ''));

const handled = new Set([...src.slice(src.indexOf('const handlers = {'), src.indexOf('window.__TAURI_INTERNALS__ = {')).matchAll(/^    (?:'([^']+)'|(\w+)):/gm)].map((m) => m[1] ?? m[2]));
const nulls = new Set([...src.match(/const NULL_COMMANDS = new Set\(\[([\s\S]*?)\]\);/)[1].matchAll(/'([^']+)'/g)].map((m) => m[1]));
const commands = [...table.matchAll(/= '([^']+)';/g)].map((m) => m[1]).filter((n) => !n.startsWith('plugin:'));

await test('answers or deliberately nulls every command in the frontend table', () => {
  assert.ok(commands.length > 60);
  assert.deepEqual(commands.filter((n) => !handled.has(n) && !nulls.has(n)), []);
});
await test('does not list a handled command as deliberately null', () => {
  assert.deepEqual([...nulls].filter((n) => handled.has(n)), []);
});
await test('does not null a command the frontend no longer has', () => {
  assert.deepEqual([...nulls].filter((n) => !commands.includes(n)), []);
});
await test('rejects a command it does not know', async () => {
  await assert.rejects(invoke('renamed_command'), /unknown Tauri command renamed_command/);
  assert.ok(sandbox.__qaLog.includes('unhandled renamed_command'));
});
await test('answers null to a command it deliberately nulls and to unknown plugin commands', async () => {
  assert.equal(await invoke('conn_wake'), null);
  assert.equal(await invoke('plugin:other|thing'), null);
});
await test('names pasted files as the shell does', async () => {
  assert.equal(await paste('image.png'), 'image.png');
  assert.equal(await paste('../../etc/passwd'), 'passwd');
  assert.equal(await paste('C:\\x\\shot 1.png'), 'shot_1.png');
  assert.equal(await paste('..'), 'paste.png');
  assert.equal(await paste(''), 'paste.png');
  assert.equal(await paste('a\nb.png'), 'a_b.png');
  assert.equal(await paste('été.png'), 'été.png');
  assert.equal(await paste('x'.repeat(100)), 'x'.repeat(80));
});
await test('decodes a malformed x-name header leniently', async () => {
  const p = await invoke('save_paste', new Uint8Array(1), { headers: { 'x-name': '100%25%zz.png' } });
  assert.ok(p.endsWith('-100__zz.png'));
});
await test('refuses pastes over the limit and non-bytes with the shell messages', async () => {
  await assert.rejects(invoke('save_paste', new Uint8Array((50 << 20) + 1), { headers: {} }), (e) => e === 'larger than the 50 MiB upload limit');
  await assert.rejects(invoke('save_paste', 'text', { headers: {} }), (e) => e === 'expected bytes');
  await assert.rejects(invoke('save_download', 'text', { headers: {} }), (e) => e === 'expected bytes');
});
await test('names downloads like pastes', async () => {
  const p = await invoke('save_download', new Uint8Array(4), { headers: { 'x-name': encodeURIComponent('../a b.txt') } });
  assert.equal(p, '/home/qa/Downloads/a_b.txt');
  assert.deepEqual({ ...sandbox.__qaDownload }, { name: 'a_b.txt', size: 4 });
});

process.exit(failed);
