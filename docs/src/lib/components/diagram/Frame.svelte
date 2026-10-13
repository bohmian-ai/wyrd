<script lang="ts">
  // The figure shell every diagram shares: what it answers and where it lives,
  // its controls, the drawing, a narration line, an optional detail
  // area, and a text version for screen readers and search.
  import type { Snippet } from 'svelte';

  let {
    answers,
    controls,
    canvas,
    narration,
    narrLabel = 'Now',
    under,
    after,
    text
  }: {
    answers: string;
    controls?: Snippet;
    canvas: Snippet;
    narration: string;
    narrLabel?: string;
    /** Rendered between the drawing and the narration, such as a scrubber. */
    under?: Snippet;
    after?: Snippet;
    text: string[];
  } = $props();
</script>

<figure class="wd">
  <div class="wd-head">
    <span class="wd-meta"><b>Answers:</b> {answers}</span>
    {#if controls}
      <div class="wd-ctrl" role="group" aria-label="Diagram controls">{@render controls()}</div>
    {/if}
  </div>
  <div class="wd-canvas">{@render canvas()}</div>
  {@render under?.()}
  <div class="wd-narr">
    <span class="wd-k">{narrLabel}</span>
    <p aria-live="polite">{@html narration}</p>
  </div>
  {@render after?.()}
  <details class="wd-text">
    <summary>Text version</summary>
    <ol>
      {#each text as line, i (i)}
        <li>{@html line}</li>
      {/each}
    </ol>
  </details>
</figure>
