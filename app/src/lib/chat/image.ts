import type { DaemonClient } from '../client';

export const chatImage = (c: DaemonClient, id: string, at: number, i: number) => c.fetchBytes(`/v1/lines/${encodeURIComponent(id)}/chat/image?at=${at}&i=${i}`);
