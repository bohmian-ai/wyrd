<script lang="ts">
  // Direction A code block (`.code`): a `.ch` header carrying the language label
  // and a copy button, over the Shiki dual-theme `<pre>`. The `html` prop is the
  // highlighter output (inline --shiki-dark props) rendered verbatim so the
  // [data-theme] dark swap in arcade.css works untouched. Copy text is read from
  // the DOM at click time, so textContent strips the token spans — clean source.
  let { html, lang = 'text', title }: { html: string; lang?: string; title?: string } = $props();

  let copied = $state(false);

  async function copy(event: MouseEvent): Promise<void> {
    const wrapper = (event.currentTarget as HTMLButtonElement).closest('.code');
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

<div class="code">
  <div class="ch">
    {#if title}<span class="fn">{title}</span>{/if}
    <span class="lang">{lang}</span>
    <button class="copy" type="button" onclick={copy}>{copied ? 'copied' : 'copy'}</button>
  </div>
  {@html html}
</div>
