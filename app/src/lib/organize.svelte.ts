import { app } from './state.svelte';
import { t } from './i18n.svelte';
import { AGENT_WAIT_MS, organizeApi, type Proposal, type Result } from './organize';

export type Phase = null | 'choice' | 'waiting' | 'review' | 'pickup';

const msg = (e: unknown) => String((e as Error)?.message ?? e);

class Organizer {
  phase = $state<Phase>(null);
  proposal = $state.raw<Proposal | null>(null);
  results = $state.raw<Result[] | null>(null);
  waitingFor = $state<string | null>(null);
  manual = $state(false);
  late = $state(false);
  busy = $state(false);
  hidden = $state.raw<string[]>([]);
  private timer: ReturnType<typeof setTimeout> | undefined;
  private off: (() => void) | null = null;

  private api() {
    if (!app.client || !app.connected) throw new Error(t('org.offline'));
    return organizeApi(app.client);
  }

  private listen() {
    this.off ??= app.onOrganize((p) => this.arrived(p));
  }

  private arrived(p: Proposal) {
    if (p.source !== 'agent' || p.status !== 'pending_review') return;
    if (this.phase === 'waiting' && p.taskId === this.waitingFor) return this.open(p);
    app.notify(t('org.ready'), 'info', { label: t('org.review'), run: () => this.open(p) });
  }

  show() {
    this.listen();
    this.late = false;
    this.manual = false;
    this.phase = 'choice';
  }

  open(p: Proposal) {
    this.stopWait();
    this.proposal = p;
    this.results = p.results ?? null;
    this.hidden = [];
    this.phase = 'review';
  }

  async quick() {
    this.busy = true;
    try {
      this.open(await this.api().deterministic());
    } catch (e) {
      app.notify(t('org.failed', { error: msg(e) }), 'error');
    } finally {
      this.busy = false;
    }
  }

  async withAgent(body: { agent: string; model?: string; effort?: string }) {
    this.listen();
    this.busy = true;
    try {
      const r = await this.api().start(body);
      this.waitingFor = r.task.id;
      this.manual = r.mcp === 'manual';
      this.late = false;
      this.phase = 'waiting';
      clearTimeout(this.timer);
      this.timer = setTimeout(() => (this.late = true), AGENT_WAIT_MS);
    } catch (e) {
      app.notify(t('org.failed', { error: msg(e) }), 'error');
    } finally {
      this.busy = false;
    }
  }

  async apply(actionIds: string[]) {
    const p = this.proposal;
    if (!p || this.busy) return;
    this.busy = true;
    try {
      const r = await this.api().apply(p.id, actionIds);
      this.results = r.results;
      this.proposal = { ...p, status: 'applied', results: r.results };
    } catch (e) {
      app.notify(t('org.failed', { error: msg(e) }), 'error');
    } finally {
      this.busy = false;
    }
  }

  async dismiss() {
    const p = this.proposal;
    this.close();
    if (p && p.status === 'pending_review' && app.connected && app.client) await this.api().dismiss(p.id).catch(() => {});
  }

  pickUp() {
    this.phase = 'pickup';
  }

  hide(lineId: string) {
    this.hidden = [...this.hidden, lineId];
  }

  private stopWait() {
    clearTimeout(this.timer);
    this.timer = undefined;
    this.waitingFor = null;
  }

  close() {
    this.stopWait();
    this.phase = null;
  }
}

export const organizer = new Organizer();
