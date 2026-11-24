<!--
  One recorded execution value, drawn the same on every page: the glyph, word
  and tone `EXECUTION_MARKS` gives it, so hue is never the only channel. A
  verdict is boxed; a value the mapping does not name reads as its own word in
  body text.
-->
<script lang="ts">
  import { EXECUTION_MARKS, type ExecutionMark } from '../observe.js';

  interface Props {
    /** A recorded status, verdict, outcome or graph word. */
    value: string;
    /** Draws the mark boxed, as a verdict. */
    badge?: boolean;
    /** Words that replace the mapped word, such as a count across items; the glyph and tone stay the value's. */
    label?: string;
    title?: string;
  }

  let { value, badge = false, label, title }: Props = $props();
  const mark: ExecutionMark = $derived(EXECUTION_MARKS[value] ?? ['', value, 'text']);
</script>

<span class="mark" class:badge data-tone={mark[2]} {title}><span aria-hidden="true">{mark[0]}</span>{label ?? mark[1]}</span>

<style>
  .mark {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: 12px var(--font-mono);
    white-space: nowrap;
  }
  .badge {
    padding: 0 6px;
    border: 2px solid currentColor;
    border-radius: var(--r);
    font: 11px/16px var(--font-mono);
  }
  [data-tone='text'] {
    color: var(--text);
  }
  [data-tone='muted'] {
    color: var(--muted);
  }
  [data-tone='run'] {
    color: var(--declare);
  }
  [data-tone='ok'] {
    color: var(--ok);
  }
  [data-tone='warn'] {
    color: var(--warn);
  }
  [data-tone='danger'] {
    color: var(--danger);
  }
</style>
