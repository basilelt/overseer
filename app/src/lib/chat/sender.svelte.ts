import type { BroadcastRequest, BroadcastResponse, LineLocal } from '../types';
import { sendRequest, sendResult, startMode, type SendMode } from './composer';

export type SendOutcome = { kind: 'none' } | { kind: 'ask' } | { kind: 'busy' } | { kind: 'sent' | 'queued'; text: string } | { kind: 'failed'; reason: string };

const message = (e: unknown) => String((e as Error)?.message ?? e);

export class ChatSender {
  sending = $state(false);
  asking = $state(false);
  queued = $state(false);

  constructor(private broadcast: (req: BroadcastRequest) => Promise<BroadcastResponse>) {}

  reset() {
    this.asking = false;
    this.queued = false;
  }

  async send(text: string, lineId: string, local: LineLocal | undefined, mode: SendMode): Promise<SendOutcome> {
    if (this.sending) return { kind: 'none' };
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
      if (out.kind === 'busy') {
        this.asking = true;
        return { kind: 'busy' };
      }
      if (out.kind === 'failed') return out;
      this.queued = out.kind === 'queued';
      this.asking = false;
      return { kind: out.kind, text };
    } catch (e) {
      return { kind: 'failed', reason: message(e) };
    } finally {
      this.sending = false;
    }
  }
}
