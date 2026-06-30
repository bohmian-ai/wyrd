<script lang="ts">
  // Renders the article content column.  Wraps children in an <article> with
  // the `.doc-content` class so heading, prose, code, and table styles from
  // wyrd.css apply without re-authoring them here.  Commit 08's archetypes
  // feed the rendered .svx body through this component.
  let { children }: { children?: import('svelte').Snippet } = $props();
</script>

<article class="doc-content" data-pagefind-body>
  {@render children?.()}
</article>
