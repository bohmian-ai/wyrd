<script lang="ts">
  import { micromark } from 'micromark';
  import { gfm, gfmHtml } from 'micromark-extension-gfm';
  let { body }: { body: string } = $props();
  // Raw HTML and unsafe protocols remain disabled. Images stay text to avoid external loads.
  let html = $derived(
    micromark(body, {
      extensions: [gfm(), { disable: { null: ['labelStartImage'] } }],
      htmlExtensions: [gfmHtml()]
    })
  );
</script>
<div class="markdown">{@html html}</div>
<style>
  .markdown {
    overflow-wrap: anywhere;
    line-height: 1.6;
  }
  .markdown :global(table) {
    display: block;
    max-width: 100%;
    overflow-x: auto;
    border-collapse: collapse;
  }
  .markdown :global(td),
  .markdown :global(th) {
    padding: 6px;
    border: 2px solid var(--border);
  }
  .markdown :global(pre) {
    overflow-x: auto;
  }
  .markdown :global(ul),
  .markdown :global(ol) {
    padding-left: 24px;
  }
</style>
