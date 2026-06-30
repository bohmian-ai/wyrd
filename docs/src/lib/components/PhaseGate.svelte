<script lang="ts">
  import type { Snippet } from 'svelte';

  // Roadmap-flagged callout: an inset card with a rune phase badge. Uses the
  // global `.wyrd-phase-gate` recipe in src/styles/wyrd.css.
  let {
    phase,
    label,
    children
  }: { phase: string; label?: string; children?: Snippet } = $props();
</script>

<aside class="wyrd-phase-gate" data-phase={phase}>
  <span class="wyrd-phase-gate__badge">{label ?? `Phase ${phase}`}</span>
  {@render children?.()}
</aside>
