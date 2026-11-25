<!--
  A block of JSON, YAML or TOML, coloured by syntax role with the brand's code
  tokens. The code field is dark in both modes. Colour is decoration only:
  the text is unchanged, selectable and copyable. A long block keeps a
  bounded height and scrolls, so machine text never pushes the page's own
  content out of view.
-->
<script lang="ts">
  import { codeTokens } from '../format.js';

  interface Props {
    /** The JSON or YAML text, or TOML when `toml` is set. */
    text: string;
    /** Read the text as TOML. */
    toml?: boolean;
  }

  let { text, toml = false }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling block must be reachable by keyboard -->
<pre class="code" tabindex="0">{#each codeTokens(text, toml) as token, index (index)}{#if token.kind === 'text'}{token.text}{:else}<span data-token={token.kind}>{token.text}</span>{/if}{/each}</pre>

<style>
  pre.code {
    margin: 0;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
    padding: 10px 12px;
    background: var(--code-bg);
    color: var(--code-text);
    font: 12px/1.55 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 20em;
    overflow-y: auto;
  }
  pre.code:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  [data-token='key'] {
    color: var(--code-keyword);
  }
  [data-token='string'] {
    color: var(--code-string);
  }
  [data-token='number'],
  [data-token='literal'] {
    color: var(--code-number);
  }
  [data-token='comment'],
  [data-token='punct'] {
    color: var(--code-muted);
  }
</style>
