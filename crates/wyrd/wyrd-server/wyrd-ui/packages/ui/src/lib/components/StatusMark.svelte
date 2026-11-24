<!--
  An execution's status or recorded verdict, drawn the same on Verify, a
  review set and the execution page: the one glyph, word and tone that
  `EXECUTION_STATUS` or `EXECUTION_VERDICT` gives the value, so hue is never
  the only channel. A verdict is boxed; a value neither map names reads as its
  own word in body text.
-->
<script lang="ts">
  import { EXECUTION_STATUS, EXECUTION_VERDICT, type ExecutionMark } from '../observe.js';

  interface Props {
    /** A retained execution status; ignored when `verdict` is given. */
    status?: string;
    /** A recorded verdict. */
    verdict?: string;
    title?: string;
  }

  let { status = '', verdict, title }: Props = $props();
  const mark: ExecutionMark = $derived(verdict === undefined ? (EXECUTION_STATUS[status] ?? ['', status, 'text']) : (EXECUTION_VERDICT[verdict] ?? ['', verdict, 'text']));
</script>

<span class="mark" class:badge={verdict !== undefined} data-tone={mark[2]} {title}><span aria-hidden="true">{mark[0]}</span>{mark[1]}</span>

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
