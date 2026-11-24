<!-- StatusPill stand-in: glyph + word + coloured 2px rule. -->
<script>
  let { tone = 'ok', children } = $props();
  const GLYPH = { ok: '✓', warn: '!', danger: '✕', info: '●', off: '–' };
</script>
<span class="pill {tone}"><span class="g" aria-hidden="true">{GLYPH[tone]}</span>{@render children()}</span>
