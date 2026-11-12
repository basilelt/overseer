export class OneShot {
  private id: ReturnType<typeof setTimeout> | null = null;

  get active(): boolean {
    return this.id !== null;
  }

  set(fn: () => void, ms: number): void {
    this.clear();
    this.id = setTimeout(() => {
      this.id = null;
      fn();
    }, ms);
  }

  clear(): void {
    if (this.id === null) return;
    clearTimeout(this.id);
    this.id = null;
  }
}
