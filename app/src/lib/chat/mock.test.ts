import { describe, expect, it } from 'vitest';
import { api } from '../api';
import { MockDaemon } from '../mock';
import type { DaemonEvent } from '../types';
import { parsePrompt } from './prompt';
import { applyTail, applyUpdate, EMPTY_CHAT } from './store';

describe('mock chat routes', () => {
  it('serves a tail, streams appended messages and answers prompts', async () => {
    const m = new MockDaemon({ live: false, delayMs: 0 });
    const events: DaemonEvent[] = [];
    m.subscribe((e) => events.push(e));
    const a = api(m);
    let s = applyTail(EMPTY_CHAT, await a.chat('ln_03', { limit: 5 }));
    expect(s.entries).toHaveLength(5);
    expect(s.more).toBe(true);
    expect(s.model).toBe('claude-opus-4');
    const res = await a.broadcast({ text: 'thanks', lineIds: ['ln_03'], onlyIdle: true });
    expect(res.results[0].result).toBe('sent');
    const ev = events.find((e) => e.type === 'line.chat')!;
    expect(ev.data).toMatchObject({ lineId: 'ln_03' });
    s = applyUpdate(s, await a.chat('ln_03', { after: s.cursor }));
    expect(s.entries.at(-1)).toMatchObject({ kind: 'user', text: 'thanks' });
    expect(parsePrompt((await a.screen('ln_01')).text, 'claude')?.options).toHaveLength(5);
    expect(parsePrompt((await a.screen('ln_02')).text, 'claude')?.options).toHaveLength(3);
    expect(await a.keys('ln_01', ['1'])).toEqual({ sent: 1 });
    expect(await a.keys('ln_01', ['down'], 'hello')).toEqual({ sent: 2 });
    expect((await a.commands('ln_01')).commands.length).toBeGreaterThan(0);
  });

  it('pages a sub-agent transcript', async () => {
    const a = api(new MockDaemon({ live: false, delayMs: 0 }));
    const page = await a.chat('ln_03', { limit: 200 });
    const card = page.entries.find((e) => e.tool?.agent?.state === 'running')!;
    const sub = await a.chat('ln_03', { limit: 200 }, card.tool!.agent!.agentId!);
    expect(sub.agentId).toBe(card.tool!.agent!.agentId);
    expect(sub.entries[0]).toMatchObject({ kind: 'user' });
    await expect(a.chat('ln_03', {}, 'missing')).rejects.toMatchObject({ status: 404 });
  });
});
