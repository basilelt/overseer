import type { Key } from '../i18n.svelte';
import type { SessionAction } from './session-state';

export interface ActionApi {
  keys(id: string, keys: string[]): Promise<unknown>;
  holdAfterTurn(id: string): Promise<{ action?: string }>;
  cancelHold(id: string): Promise<unknown>;
  close(id: string, confirm: boolean): Promise<unknown>;
  resume(id: string): Promise<unknown>;
}

export interface ActionResult {
  notice?: Key;
  term?: unknown;
  dropChat?: true;
}

/** One session action on the daemon; errors propagate so the view can show them. */
export async function runSessionAction(api: ActionApi, id: string, kind: SessionAction): Promise<ActionResult> {
  switch (kind) {
    case 'interrupt':
      await api.keys(id, ['esc']);
      return {};
    case 'pause': {
      // afterTurn holds an idle agent at once and queues the hold while a turn runs (a plain hold is refused mid-turn).
      const plan = await api.holdAfterTurn(id);
      return { notice: plan.action === 'hold-pending' ? 'session.state.pause_pending' : 'chat.paused' };
    }
    case 'cancelPause':
      await api.cancelHold(id);
      return { notice: 'chat.pauseCancelled' };
    case 'close':
      await api.close(id, true);
      return { notice: 'chat.closed', dropChat: true };
    case 'resume':
      return { term: await api.resume(id) };
  }
}
