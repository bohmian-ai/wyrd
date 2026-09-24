<script lang="ts">
  // Wraps a diagram figure. Inline it renders once (server-rendered, visible
  // without JS). Zoomed, it lifts into a fullscreen stage with real pan and
  // zoom: scroll to scale, drag to pan. Native element/image dragging is
  // disabled so a grab pans instead of spawning a browser drag ghost. The child
  // is rendered in exactly one branch at a time, so marker ids stay unique.
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();

  let zoomed = $state(false);
  let scale = $state(1);
  let tx = $state(0);
  let ty = $state(0);
  let dragging = $state(false);

  const MIN = 1;
  const MAX = 6;

  // Pointer-drag origin.
  let px = 0;
  let py = 0;
  let ox = 0;
  let oy = 0;

  function open() {
    reset();
    zoomed = true;
  }
  function close() {
    zoomed = false;
  }
  function reset() {
    scale = 1;
    tx = 0;
    ty = 0;
  }

  function onKey(e: KeyboardEvent) {
    if (zoomed && e.key === 'Escape') close();
  }

  function onPointerDown(e: PointerEvent) {
    dragging = true;
    px = e.clientX;
    py = e.clientY;
    ox = tx;
    oy = ty;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    tx = ox + (e.clientX - px);
    ty = oy + (e.clientY - py);
  }
  function onPointerUp(e: PointerEvent) {
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  }
  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const next = Math.min(MAX, Math.max(MIN, scale * (1 - e.deltaY * 0.0015)));
    if (next === MIN) {
      reset();
    } else {
      scale = next;
    }
  }

  // Lock background scroll while the overlay is open.
  $effect(() => {
    if (typeof document === 'undefined') return;
    document.body.style.overflow = zoomed ? 'hidden' : '';
    return () => {
      document.body.style.overflow = '';
    };
  });
</script>

<svelte:window onkeydown={onKey} />

{#if zoomed}
  <button class="zf-backdrop" aria-label="Close zoomed diagram" onclick={close}></button>
  <div class="zf-stage">
    <div class="zf-bar">
      <span class="zf-hint">scroll to zoom · drag to pan · double-click to reset</span>
      <button class="zf-btn" onclick={close}>CLOSE</button>
    </div>
    <div
      class="zf-viewport"
      class:dragging
      role="presentation"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onwheel={onWheel}
      ondblclick={reset}
      ondragstart={(e) => e.preventDefault()}
    >
      <div class="zf-pan" style={`transform: translate(${tx}px, ${ty}px) scale(${scale});`}>
        {@render children()}
      </div>
    </div>
  </div>
{:else}
  <div class="zf">
    <button class="zf-btn zf-btn-open" onclick={open}>ZOOM</button>
    {@render children()}
  </div>
{/if}

<style>
  .zf {
    position: relative;
  }
  .zf-btn {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 5px 8px;
    cursor: pointer;
    transition: background-color 160ms ease-out;
  }
  .zf-btn:hover {
    background: var(--surface-hover);
  }
  .zf-btn-open {
    position: absolute;
    top: 10px;
    right: 10px;
    z-index: 2;
  }

  .zf-backdrop {
    position: fixed;
    inset: 0;
    z-index: 999;
    background: color-mix(in srgb, var(--bg) 72%, transparent);
    border: none;
    cursor: zoom-out;
  }

  .zf-stage {
    position: fixed;
    inset: 4vh 3vw;
    z-index: 1000;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r);
    overflow: hidden;
  }
  .zf-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--border);
  }
  .zf-hint {
    font-family: var(--font-mono);
    font-size: 0.66rem;
    letter-spacing: 0.03em;
    color: var(--muted);
  }

  .zf-viewport {
    flex: 1;
    display: grid;
    place-items: center;
    overflow: hidden;
    cursor: grab;
    touch-action: none;
    -webkit-user-select: none;
    user-select: none;
  }
  .zf-viewport.dragging {
    cursor: grabbing;
  }
  .zf-pan {
    width: 100%;
    transform-origin: center center;
    will-change: transform;
  }
  .zf-pan :global(figure) {
    margin: 0;
    border: none;
    overflow: visible;
    pointer-events: none;
  }
  .zf-pan :global(svg) {
    -webkit-user-drag: none;
  }

  @media (max-width: 640px) {
    .zf-hint {
      display: none;
    }
  }
</style>
