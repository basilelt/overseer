import type { BroadcastRequest, BroadcastResponse, LineLocal } from '../types';
import { sendRequest, sendResult, startMode, type SendMode } from './composer';

export type SendOutcome = { kind: 'none' } | { kind: 'ask' } | { kind: 'busy' } | { kind: 'sent' | 'queued'; text: string } | { kind: 'failed'; reason: string };

const message = (e: unknown) => String((e as Error)?.message ?? e);

export class ChatSender {
  sending = $state(false);
  asking = $state(false);
  queued = $state(false);
  private line = '';

  constructor(private broadcast: (req: BroadcastRequest) => Promise<BroadcastResponse>) {}

  reset(lineId: string) {
    this.line = lineId;
    this.asking = false;
    this.queued = false;
  }

  async send(text: string, lineId: string, local: LineLocal | undefined, mode: SendMode): Promise<SendOutcome> {
    if (this.sending) return { kind: 'none' };
    this.line = lineId;
    if (mode === 'idle' && startMode(local) === 'ask' && !this.asking) {
      if (!text.trim()) return { kind: 'none' };
      this.asking = true;
      return { kind: 'ask' };
    }
    const req = sendRequest(text, lineId, mode);
    if (!req) return { kind: 'none' };
    this.sending = true;
    try {
      const out = sendResult((await this.broadcast(req)).results);
      const here = this.line === lineId;
      if (out.kind === 'busy') {
        if (here) this.asking = true;
        return { kind: 'busy' };
      }
      if (out.kind === 'failed') return out;
      if (here) {
        this.queued = out.kind === 'queued';
        this.asking = false;
      }
      return { kind: out.kind, text };
    } catch (e) {
      return { kind: 'failed', reason: message(e) };
    } finally {
      this.sending = false;
    }
  }
}
