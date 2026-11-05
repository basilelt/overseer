export class Lru<K, V> {
  private map = new Map<K, { value: V; weight: number }>();
  private total = 0;

  constructor(
    private max: number,
    private budget = Infinity,
    private weigh: (v: V) => number = () => 1,
  ) {}

  get size(): number {
    return this.map.size;
  }

  get weight(): number {
    return this.total;
  }

  has(k: K): boolean {
    return this.map.has(k);
  }

  get(k: K): V | undefined {
    const e = this.map.get(k);
    if (!e) return undefined;
    this.map.delete(k);
    this.map.set(k, e);
    return e.value;
  }

  set(k: K, value: V): boolean {
    this.delete(k);
    const weight = this.weigh(value);
    if (weight > this.budget) return false;
    this.map.set(k, { value, weight });
    this.total += weight;
    for (const [old, e] of this.map) {
      if (this.map.size <= this.max && this.total <= this.budget) break;
      this.map.delete(old);
      this.total -= e.weight;
    }
    return true;
  }

  delete(k: K) {
    const e = this.map.get(k);
    if (!e) return;
    this.map.delete(k);
    this.total -= e.weight;
  }

  clear() {
    this.map.clear();
    this.total = 0;
  }
}
