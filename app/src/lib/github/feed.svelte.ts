import { ghError } from './format';
import { githubApi } from './api';
import type { ReviewRequests } from './types';

export const FOCUS_REFRESH_MS = 60_000;

type GhError = ReturnType<typeof ghError>;

export interface FeedClient {
  request<T>(method: string, path: string): Promise<T>;
}

export class ReviewFeed {
  data = $state<ReviewRequests | null>(null);
  error = $state<GhError | null>(null);
  loading = $state(false);
  private gen = 0;
  private host: string | null = null;
  private at = -Infinity;

  constructor(private now: () => number = Date.now) {}

  async load(client: FeedClient, host: string, refresh = false): Promise<GhError | null> {
    if (host !== this.host) {
      this.host = host;
      this.data = null;
      this.error = null;
      this.at = -Infinity;
    }
    const gen = ++this.gen;
    this.loading = true;
    try {
      const data = await githubApi(client).reviewRequests(refresh);
      if (gen !== this.gen) return null;
      this.data = data;
      this.error = null;
      this.at = this.now();
      return null;
    } catch (e) {
      if (gen !== this.gen) return null;
      this.error = ghError(e);
      return this.data?.prs.length ? this.error : null;
    } finally {
      if (gen === this.gen) this.loading = false;
    }
  }

  due(minMs = FOCUS_REFRESH_MS): boolean {
    return !this.loading && this.now() - this.at >= minMs;
  }
}
