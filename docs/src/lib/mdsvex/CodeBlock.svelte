<script lang="ts">
  // Thin client-only copy wrapper over Shiki-highlighted HTML. The `html` prop
  // carries the dual-theme output (inline --shiki-dark props); we render it
  // verbatim so theme switching via [data-theme] CSS works without interference.
  // Text is read from the DOM at click time so inline style attrs are stripped
  // automatically by textContent — the user copies clean source, not ANSI spans.
  let { html }: { html: string } = $props();

  let copied = $state(false);

  async function copy(event: MouseEvent): Promise<void> {
    const wrapper = (event.currentTarget as HTMLButtonElement).closest('.cb');
    const pre = wrapper?.querySelector('pre');
    const text = pre?.textContent ?? '';
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard unavailable — silent fail */
    }
  }
</script>

<div class="cb">
  <button class="cb-copy" type="button" onclick={copy}>{copied ? 'copied' : 'copy'}</button>
  {@html html}
</div>

<style>
  .cb {
    position: relative;
    margin: 1.5rem 0;
  }
  .cb-copy {
    position: absolute;
    top: 8px;
    right: 10px;
    z-index: 1;
    font-family: var(--font-mono);
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    background: var(--surface-2);
    border: 2px solid var(--border);
    border-radius: 4px;
    padding: 2px 7px;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.1s;
  }
  .cb:hover .cb-copy {
    opacity: 1;
  }
  .cb-copy:hover {
    color: var(--text);
  }
  /* Shiki pre inherits the wrapper's block context. */
  .cb :global(pre.shiki) {
    margin: 0;
    padding: 12px 14px;
    overflow-x: auto;
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    font-family: var(--font-mono);
    font-size: 0.82rem;
    line-height: 1.55;
  }
</style>
