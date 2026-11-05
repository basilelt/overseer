<script lang="ts">
  import { clampWidth, dragWidth, type Bounds } from '../../resize';

  let { width, bounds, side, label, onchange, oncommit }: { width: number; bounds: Bounds; side: 'left' | 'right'; label: string; onchange: (w: number) => void; oncommit: (w: number) => void } = $props();

  let drag: { x: number; w: number } | null = null;

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { x: e.clientX, w: width };
  }

  function move(e: PointerEvent) {
    if (drag) onchange(dragWidth(drag.w, e.clientX - drag.x, side, bounds));
  }

  function up() {
    if (!drag) return;
    drag = null;
    oncommit(width);
  }

  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 48 : 16;
    const dir = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const w = clampWidth(width + (side === 'left' ? dir : -dir) * step, bounds);
    onchange(w);
    oncommit(w);
  }

  function reset() {
    onchange(bounds.initial);
    oncommit(bounds.initial);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="resizer"
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={width}
  aria-valuemin={bounds.min}
  aria-valuemax={bounds.max}
  tabindex="0"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onkeydown={key}
  ondblclick={reset}
></div>

<style>
  .resizer {
    flex: none;
    width: 6px;
    margin: 0 -3px;
    position: relative;
    z-index: 2;
    cursor: col-resize;
    touch-action: none;
  }
  .resizer::after {
    content: '';
    position: absolute;
    inset: 0 2px;
    background: transparent;
    transition: background 0.12s;
  }
  .resizer:hover::after,
  .resizer:focus-visible::after,
  .resizer:active::after {
    background: var(--accent);
  }
  .resizer:focus-visible {
    outline: none;
  }
</style>
