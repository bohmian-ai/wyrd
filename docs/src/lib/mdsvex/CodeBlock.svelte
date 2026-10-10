<script module lang="ts">
  export type CopyState = 'Copy' | 'Copied' | 'Copy failed';

  // Write the sample to the clipboard and name the outcome for the live region.
  export async function copyText(
    text: string,
    clipboard: Pick<Clipboard, 'writeText'> = navigator.clipboard
  ): Promise<CopyState> {
    try {
      await clipboard.writeText(text);
      return 'Copied';
    } catch {
      return 'Copy failed';
    }
  }
</script>

<script lang="ts">
  // Code sample: a label/copy header over the Shiki `<pre>` inside a focusable
  // horizontal scroll region. `html` is highlighter output rendered verbatim.
  // Copy text is read from the DOM at click time, so token spans are stripped.
  let { html, lang = 'text', title }: { html: string; lang?: string; title?: string } = $props();

  let state = $state<CopyState>('Copy');
  let scroller: HTMLDivElement;

  async function copy(): Promise<void> {
    state = await copyText(scroller.querySelector('pre')?.textContent ?? '');
    setTimeout(() => (state = 'Copy'), 1600);
  }
</script>

<div class="code">
  <div class="code-head">
    {#if title}<span class="code-file">{title}</span>{/if}
    <span class="code-lang">{lang}</span>
    <button class="copy" type="button" data-state={state} onclick={copy}>
      <svg viewBox="0 0 16 16" aria-hidden="true">
        {#if state === 'Copied'}
          <path d="m3 8 3 3 7-7" fill="none" stroke="currentColor" stroke-width="1.6" />
        {:else}
          <rect x="5" y="5" width="8" height="8" fill="none" stroke="currentColor" />
          <path d="M3 11H2V2h9v1" fill="none" stroke="currentColor" />
        {/if}
      </svg>
      <span aria-live="polite">{state}</span>
    </button>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="code-scroll"
    tabindex="0"
    role="region"
    aria-label={`${title ?? lang} code`}
    bind:this={scroller}
  >
    {@html html}
  </div>
</div>
