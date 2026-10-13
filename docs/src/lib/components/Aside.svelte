<script lang="ts">
  import type { Snippet } from 'svelte';

  // Callout (`.aside` in docs.css). `type` sets the rule color and, for caution
  // and danger, a glyph before the label so meaning never rests on color alone.
  // Content may still author `data-variant="not"`, which reads as caution.
  type AsideType = 'note' | 'tip' | 'caution' | 'danger';

  let {
    title,
    type,
    children,
    ...rest
  }: {
    title?: string;
    type?: AsideType;
    children?: Snippet;
    [key: string]: unknown;
  } = $props();

  const resolvedType = $derived<AsideType>(
    type ?? (rest['data-variant'] === 'not' ? 'caution' : 'note')
  );
  const label = $derived(title ?? resolvedType[0].toUpperCase() + resolvedType.slice(1));
</script>

<div class="aside" data-type={resolvedType} {...rest}>
  <div class="at">{label}</div>
  {@render children?.()}
</div>
