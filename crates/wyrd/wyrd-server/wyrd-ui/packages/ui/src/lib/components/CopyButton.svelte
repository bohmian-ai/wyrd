<!-- Copies one text to the clipboard and says so on the button. Local feedback; copying is not a host action. -->
<script lang="ts">
  let { label, text }: { label: string; text: string | (() => string) } = $props();

  let copied = $state<'Copied' | 'Copy failed'>();
  async function copy() {
    try {
      await navigator.clipboard.writeText(typeof text === 'function' ? text() : text);
      copied = 'Copied';
    } catch {
      copied = 'Copy failed';
    }
    setTimeout(() => (copied = undefined), 1500);
  }
</script>

<button type="button" class="btn btn-sm" class:btn-copied={copied === 'Copied'} onclick={copy}><span aria-live="polite">{copied ?? label}</span></button>
