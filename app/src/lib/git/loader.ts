import type { GitApi } from './api';
import { EMPTY_STATE, layout, type LayoutState, type Row } from './layout';
import type { GraphCommit, GraphPage, RefsFilter } from './types';

export interface GraphSnapshot {
  page: GraphPage | null;
  commits: GraphCommit[];
  rows: Row[];
  next?: string;
}

export const EMPTY_GRAPH: GraphSnapshot = { page: null, commits: [], rows: [] };

export class GraphLoader {
  snap: GraphSnapshot = EMPTY_GRAPH;
  private state: LayoutState = EMPTY_STATE;
  private seq = 0;
  private inflight: Promise<void> | null = null;

  constructor(
    private api: GitApi,
    public project: string,
    public refs: RefsFilter = 'all',
    public pageSize = 400,
  ) {}

  async load(limit = this.pageSize): Promise<GraphSnapshot> {
    const seq = ++this.seq;
    const page = await this.api.graph(this.project, { limit: Math.min(2000, limit), refs: this.refs });
    if (seq !== this.seq) return this.snap;
    const { rows, state } = layout(page.commits);
    this.state = state;
    this.snap = { page, commits: page.commits, rows, next: page.next };
    return this.snap;
  }

  reload(): Promise<GraphSnapshot> {
    return this.load(Math.max(this.pageSize, this.snap.commits.length));
  }

  async more(): Promise<GraphSnapshot> {
    if (!this.snap.next) return this.snap;
    if (this.inflight) {
      await this.inflight;
      return this.snap;
    }
    const seq = this.seq;
    const before = this.snap.next;
    this.inflight = (async () => {
      const page = await this.api.graph(this.project, { limit: this.pageSize, before, refs: this.refs });
      if (seq !== this.seq || before !== this.snap.next) return;
      const { rows, state } = layout(page.commits, this.state);
      this.state = state;
      this.snap = { page: { ...page, commits: [] }, commits: [...this.snap.commits, ...page.commits], rows: [...this.snap.rows, ...rows], next: page.next };
    })();
    try {
      await this.inflight;
    } finally {
      this.inflight = null;
    }
    return this.snap;
  }
}
