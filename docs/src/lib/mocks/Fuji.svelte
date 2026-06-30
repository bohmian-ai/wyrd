<script lang="ts">
  let { size = 24, class: cls = '' }: { size?: number; class?: string } = $props();
</script>

<svg
  class={`fuji ${cls}`}
  width={size}
  height={size}
  viewBox="0 0 100 100"
  aria-hidden="true"
>
  <path class="p" d="M42 92 L30 92 L6 8 L22 8 Z" />
  <path class="p" d="M58 92 L70 92 L94 8 L78 8 Z" />
  <path class="c" d="M44 8 h12 v84 h-12 z" />
</svg>
