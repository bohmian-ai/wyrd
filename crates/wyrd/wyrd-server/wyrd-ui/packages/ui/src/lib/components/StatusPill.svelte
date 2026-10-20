<!--
  A status badge. Tone drives border, label and tint; every non-neutral tone
  carries a glyph so status never rests on hue alone.
-->
<script lang="ts" module>
  /** The status a pill reports; `neutral` means no status. */
  export type Tone = 'neutral' | 'ok' | 'warn' | 'danger' | 'running';
</script>

<script lang="ts">
  interface Props {
    tone?: Tone;
    label: string;
  }

  let { tone = 'neutral', label }: Props = $props();

  const GLYPHS: Record<Tone, string> = { neutral: '', ok: '✓', warn: '!', danger: '✕', running: '●' };
</script>

<span class="pill" data-tone={tone}>
  {#if GLYPHS[tone]}<span aria-hidden="true">{GLYPHS[tone]}</span>{/if}{label}
</span>

<style>
  .pill {
    --bc: var(--muted);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 1px solid var(--bc);
    border-radius: var(--r);
    padding: 1px 6px;
    color: var(--bc);
    font: var(--weight-strong) 11px/16px var(--font-mono);
    white-space: nowrap;
  }
  .pill:not([data-tone='neutral']) {
    background: color-mix(in srgb, var(--bc) 12%, transparent);
  }
  [data-tone='neutral'] {
    color: var(--text);
    border-color: var(--border);
  }
  [data-tone='ok'] {
    --bc: var(--ok);
  }
  [data-tone='warn'] {
    --bc: var(--warn);
  }
  [data-tone='danger'] {
    --bc: var(--danger);
  }
  [data-tone='running'] {
    --bc: var(--declare);
  }
</style>
