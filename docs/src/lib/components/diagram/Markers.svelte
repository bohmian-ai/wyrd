<svelte:options namespace="svg" />

<script lang="ts">
  // Arrowheads for one SVG. Ids carry the instance prefix so diagrams on the
  // same page never share a marker.
  let { id }: { id: string } = $props();
</script>

<defs>
  {#each ['data', 'control', 'ref'] as k (k)}
    <marker id={`${id}-${k}`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
      <path d="M0 0 L10 5 L0 10 z" class={`wd-mk-${k}`} />
    </marker>
  {/each}
</defs>
