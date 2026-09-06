<script lang="ts">
  import type { Snippet } from 'svelte';

  // Status is never carried by colour alone: --ok and --danger collapse to 1.11:1 under
  // deuteranopia, so every non-neutral tone pairs a glyph with its text label. `neutral`
  // means "no status" and therefore takes neither a glyph nor a tint.
  type Tone = 'neutral' | 'ok' | 'warn' | 'danger' | 'running';
  let { tone = 'neutral', children }: { tone?: Tone; children?: Snippet } = $props();

  const glyphs: Record<Tone, string> = {
    neutral: '',
    ok: '✓',
    warn: '!',
    danger: '✕',
    running: '●'
  };
</script>

<span class="wy-badge" data-tone={tone}>
  {#if glyphs[tone]}<span class="gl" aria-hidden="true">{glyphs[tone]}</span>{/if}{@render children?.()}
</span>

<style>
  .wy-badge {
    /* --bc is always a *-text token: raw --ok/--warn/--danger are fills and land at
       3.01 / 2.28 / 3.94:1 as an 8.5px label on light. */
    --bc: var(--muted);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--fm);
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    padding: 3px 7px;
    border: 2px solid var(--bc);
    border-radius: 4px;
    color: var(--bc);
    background: color-mix(in srgb, var(--bc) 12%, var(--surface));
  }
  /* neutral takes no tint: a 12% --muted wash lands at 4.46:1 in dark */
  .wy-badge[data-tone='neutral'] {
    background: var(--surface);
  }
  .wy-badge[data-tone='ok'] {
    --bc: var(--ok-text);
  }
  .wy-badge[data-tone='warn'] {
    --bc: var(--warn-text);
  }
  .wy-badge[data-tone='danger'] {
    --bc: var(--danger-text);
  }
  .wy-badge[data-tone='running'] {
    --bc: var(--brand-strong);
  }
  .gl {
    font-size: 9px;
    line-height: 1;
  }
</style>
