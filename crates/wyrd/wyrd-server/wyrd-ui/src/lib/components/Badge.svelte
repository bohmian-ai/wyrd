<script lang="ts">
  import type { Snippet } from 'svelte';

  type Tone = 'neutral' | 'ok' | 'warn' | 'danger';
  let { tone = 'neutral', children }: { tone?: Tone; children?: Snippet } = $props();
</script>

<span class="wy-badge" data-tone={tone}>{@render children?.()}</span>

<style>
  .wy-badge {
    --bc: var(--muted);
    display: inline-block;
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
  .wy-badge[data-tone='ok'] {
    --bc: var(--ok);
  }
  .wy-badge[data-tone='warn'] {
    --bc: var(--warn);
  }
  .wy-badge[data-tone='danger'] {
    --bc: var(--danger);
  }
</style>
