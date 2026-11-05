<script lang="ts" generics="T extends string">
  let { value, options, label, onchange }: { value: T; options: { value: T; label: string }[]; label: string; onchange: (v: T) => void } = $props();

  function onKey(e: KeyboardEvent) {
    const i = options.findIndex((o) => o.value === value);
    const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const next = options[(i + step + options.length) % options.length];
    onchange(next.value);
    ((e.currentTarget as HTMLElement).querySelectorAll('button')[options.indexOf(next)] as HTMLElement)?.focus();
  }
</script>

<div class="seg" role="radiogroup" aria-label={label} tabindex="-1" onkeydown={onKey}>
  {#each options as o (o.value)}
    <button type="button" role="radio" aria-checked={o.value === value} tabindex={o.value === value ? 0 : -1} onclick={() => onchange(o.value)}>
      {o.label}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: flex;
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  button {
    flex: 1;
    height: 24px;
    padding: 0 10px;
    border-radius: 4px;
    color: var(--text-2);
    font-size: 12px;
    transition:
      background var(--ease),
      color var(--ease);
  }
  button:hover {
    color: var(--text);
  }
  button[aria-checked='true'] {
    background: var(--surface);
    color: var(--text);
    font-weight: 500;
    box-shadow: 0 0 0 1px var(--border);
  }
</style>
