<script lang="ts">
  let { code, copy = true }: { code: string; copy?: boolean } = $props();

  let copied = $state(false);

  async function doCopy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(code);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard unavailable (e.g. insecure context) — no-op */
    }
  }
</script>

<div class="wy-code">
  {#if copy}
    <button class="copy" type="button" onclick={doCopy}>{copied ? 'copied' : 'copy'}</button>
  {/if}
  <pre><code>{code}</code></pre>
</div>

<style>
  .wy-code {
    position: relative;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface-2);
    overflow: hidden;
  }
  pre {
    margin: 0;
    padding: 9px 11px;
    overflow: auto;
    max-height: 240px;
  }
  code {
    font-family: var(--fm);
    font-size: 11px;
    line-height: 1.5;
    color: var(--text);
    white-space: pre;
  }
  .copy {
    position: absolute;
    top: 6px;
    right: 6px;
    font-family: var(--fm);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    padding: 3px 7px;
    border: 2px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    color: var(--muted);
    cursor: pointer;
  }
  .copy:hover {
    color: var(--text);
  }
</style>
