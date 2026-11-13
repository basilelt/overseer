export class Flights {
  private busy = new Set<string>();

  async run(key: string, fn: () => Promise<void>): Promise<boolean> {
    if (this.busy.has(key)) return false;
    this.busy.add(key);
    try {
      await fn();
      return true;
    } finally {
      this.busy.delete(key);
    }
  }
}
