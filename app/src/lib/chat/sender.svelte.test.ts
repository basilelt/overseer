import { beforeEach, describe, expect, it } from 'vitest';
import { MockDaemon } from '../daemon/mock';
import type { BroadcastRequest, BroadcastResponse, LocalLine } from '../types';
import { ChatSender } from './sender.svelte';

describe('chat sender', () => {
  let m: MockDaemon;
  let line: LocalLine;
  let sender: ChatSender;
  const delivered = () => m.delivered.map((d) => d.text);
  const send = (text: string, mode: 'idle' | 'queue' | 'now' = 'idle') => sender.send(text, line.line.id, line.local, mode);
  const busy = (on: boolean) => (line.local = { ...line.local!, busy: on });

  beforeEach(() => {
    m = new MockDaemon({ live: false, delayMs: 0 });
    line = m.lines.find((l) => l.local && !l.local.busy && !l.local.needsInput && l.line.state !== 'held' && l.line.state !== 'hung-up')!;
    sender = new ChatSender((req) => m.request<BroadcastResponse>('POST', '/v1/broadcast', req));
  });

  it('sends the text in the box once the busy prompt was left for an edited message', async () => {
    expect(await send('r1')).toEqual({ kind: 'sent', text: 'r1' });
    busy(true);
    expect(await send('r2')).toEqual({ kind: 'ask' });
    busy(false);
    expect(await send('r3')).toEqual({ kind: 'sent', text: 'r3' });
    busy(true);
    expect(await send('r4')).toEqual({ kind: 'ask' });
    expect(await send('r4', 'queue')).toEqual({ kind: 'queued', text: 'r4' });
    expect(delivered()).toEqual(['r1', 'r3']);
    busy(false);
    m.drainQueue();
    expect(delivered()).toEqual(['r1', 'r3', 'r4']);
  });

  it('queues or sends now the text currently in the box, not the one that raised the prompt', async () => {
    busy(true);
    expect(await send('first')).toEqual({ kind: 'ask' });
    expect(await send('edited', 'now')).toEqual({ kind: 'sent', text: 'edited' });
    expect(delivered()).toEqual(['edited']);
    expect(sender.asking).toBe(false);
  });

  it('keeps the prompt and sends nothing when Enter retries while the agent is still busy', async () => {
    busy(true);
    expect(await send('hi')).toEqual({ kind: 'ask' });
    expect(await send('hi')).toEqual({ kind: 'busy' });
    expect(sender.asking).toBe(true);
    expect(delivered()).toEqual([]);
  });

  it('asks when a send only when idle is skipped by the daemon', async () => {
    const s = new ChatSender(async () => ({ id: 'b', results: [{ target: `line:${line.line.id}`, result: 'skipped', reason: 'busy' }] }));
    expect(await s.send('hi', line.line.id, line.local, 'idle')).toEqual({ kind: 'busy' });
    expect(s.asking).toBe(true);
  });

  it('ignores an empty message and does not raise the prompt for it', async () => {
    expect(await send('   ')).toEqual({ kind: 'none' });
    busy(true);
    expect(await send('  ')).toEqual({ kind: 'none' });
    expect(sender.asking).toBe(false);
    expect(delivered()).toEqual([]);
  });

  it('ignores a second send while one is in flight so nothing goes twice', async () => {
    let release!: () => void;
    const reqs: BroadcastRequest[] = [];
    const s = new ChatSender(async (req) => {
      reqs.push(req);
      await new Promise<void>((r) => (release = r));
      return { id: 'b', results: [{ target: 'line:x', result: 'sent' }] };
    });
    const first = s.send('one', 'x', undefined, 'idle');
    expect(s.sending).toBe(true);
    expect(await s.send('one', 'x', undefined, 'idle')).toEqual({ kind: 'none' });
    release();
    expect(await first).toEqual({ kind: 'sent', text: 'one' });
    expect(reqs).toHaveLength(1);
    expect(s.sending).toBe(false);
  });

  it('reports a refused delivery and a daemon error without clearing the prompt state', async () => {
    line.line.state = 'hung-up';
    expect(await send('hi')).toEqual({ kind: 'failed', reason: 'not running' });
    const down = new ChatSender(() => Promise.reject(new Error('daemon down')));
    expect(await down.send('hi', 'x', undefined, 'idle')).toEqual({ kind: 'failed', reason: 'daemon down' });
    expect(down.sending).toBe(false);
  });

  it('marks a queued message and clears that mark on reset', async () => {
    busy(true);
    expect(await send('later', 'queue')).toEqual({ kind: 'queued', text: 'later' });
    expect(sender.queued).toBe(true);
    await send('again');
    sender.reset('other');
    expect(sender.asking).toBe(false);
    expect(sender.queued).toBe(false);
  });
  const held = () => {
    let release!: (r: BroadcastResponse) => void;
    const s = new ChatSender(() => new Promise<BroadcastResponse>((r) => (release = r)));
    return { s, reply: (result: 'sent' | 'queued' | 'skipped') => release({ id: 'b', results: [{ target: 'line:a', result }] }) };
  };

  it('does not show the queued note on the line the user switched to while queueing', async () => {
    const { s, reply } = held();
    s.reset('a');
    const out = s.send('later', 'a', undefined, 'queue');
    s.reset('b');
    reply('queued');
    expect(await out).toEqual({ kind: 'queued', text: 'later' });
    expect(s.queued).toBe(false);
    expect(s.asking).toBe(false);
  });

  it('does not raise the busy prompt on the line the user switched to', async () => {
    const { s, reply } = held();
    s.reset('a');
    const out = s.send('hi', 'a', undefined, 'idle');
    s.reset('b');
    reply('skipped');
    expect(await out).toEqual({ kind: 'busy' });
    expect(s.asking).toBe(false);
  });

  it('keeps the prompt of the line it now shows when an earlier send lands', async () => {
    const { s, reply } = held();
    s.reset('a');
    const out = s.send('hi', 'a', undefined, 'now');
    s.reset('b');
    s.asking = true;
    reply('sent');
    expect(await out).toEqual({ kind: 'sent', text: 'hi' });
    expect(s.asking).toBe(true);
  });
});
