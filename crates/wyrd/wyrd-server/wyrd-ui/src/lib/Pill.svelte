<!--
  An administration status: a glyph and a word with a 2px coloured rule, so
  state never rests on colour alone. Styled by console.css.
-->
<script lang="ts">
  type Tone = 'ok' | 'warn' | 'danger' | 'info' | 'off';
  let { tone = 'ok', label }: { tone?: Tone; label: string } = $props();
  const GLYPH: Record<Tone, string> = { ok: '✓', warn: '!', danger: '✕', info: '●', off: '–' };
</script>

<span class="pill {tone}"><span class="g" aria-hidden="true">{GLYPH[tone]}</span>{label}</span>
