<script lang="ts">
  // The Wyrd mark: exact README geometry (brand/gen-theme.mjs owns it), recolored
  // by the docs theme — wings --declare, spine --evidence, outline --text in
  // light and --bg in dark. Recolor, never redraw.
  let { size = 24, label = 'Wyrd' }: { size?: number; label?: string } = $props();
</script>

<svg
  class="wyrd-mark"
  viewBox="0 0 100 100"
  width={size}
  height={size}
  role="img"
  aria-label={label}
  xmlns="http://www.w3.org/2000/svg"
>
  <path class="w" d="M42 92 L30 92 L6 8 L22 8 Z" stroke-width="2" />
  <path class="w" d="M58 92 L70 92 L94 8 L78 8 Z" stroke-width="2" />
  <path class="s" d="M44 8 h12 v84 h-12 z" stroke-width="2" />
</svg>

<style>
  .wyrd-mark {
    display: block;
    flex: 0 0 auto;
  }
  .wyrd-mark path {
    stroke: var(--text);
  }
  :global(:root[data-theme='dark']) .wyrd-mark path {
    stroke: var(--bg);
  }
  .w {
    fill: var(--declare);
  }
  .s {
    fill: var(--evidence);
  }
</style>
