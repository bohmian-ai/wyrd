<script lang="ts">
  // The Wyrd runic "fuji" mark — violet prongs, lime spine. Colors via tokens
  // (no hardcoded hex), so it tracks light/dark like everything else.
  let { size = 20 }: { size?: number } = $props();
</script>

<svg
  class="wyrd-mark"
  viewBox="0 0 100 100"
  width={size}
  height={size}
  role="img"
  aria-label="Wyrd"
  xmlns="http://www.w3.org/2000/svg"
>
  <path class="p" d="M42 92 L30 92 L6 8 L22 8 Z" />
  <path class="p" d="M58 92 L70 92 L94 8 L78 8 Z" />
  <path class="c" d="M44 8 h12 v84 h-12 z" />
</svg>

<style>
  .wyrd-mark {
    display: block;
    flex: 0 0 auto;
  }
  .wyrd-mark .p {
    fill: var(--rune-strong);
  }
  .wyrd-mark .c {
    fill: var(--lime);
  }
</style>
