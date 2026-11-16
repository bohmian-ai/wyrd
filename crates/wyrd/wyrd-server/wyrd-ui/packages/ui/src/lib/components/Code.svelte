<!--
  A block of JSON or YAML, coloured by syntax role with the brand's code
  tokens. The code field is dark in both modes. Colour is decoration only:
  the text is unchanged, selectable and copyable.
-->
<script lang="ts">
  import { codeTokens } from '../format.js';

  interface Props {
    /** The JSON or YAML text. */
    text: string;
  }

  let { text }: Props = $props();
</script>

<pre class="code">{#each codeTokens(text) as token, index (index)}{#if token.kind === 'text'}{token.text}{:else}<span data-token={token.kind}>{token.text}</span>{/if}{/each}</pre>

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
  [data-token='punct'] {
    color: var(--code-muted);
  }
</style>
