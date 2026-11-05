<!-- A verification verdict as a StatusPill: the word and glyph carry the result. -->
<script lang="ts">
  import type { CardTypes } from 'wyrd-ai';
  import StatusPill, { type Tone } from './StatusPill.svelte';

  let { verdict }: { verdict: CardTypes.VerificationVerdict } = $props();

  const TONES: Record<CardTypes.VerificationVerdict, Tone> = { passed: 'ok', failed: 'danger', inconclusive: 'warn' };
</script>

<StatusPill tone={TONES[verdict]} label={verdict} />
