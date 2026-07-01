<script lang="ts">
  import type { Snippet } from 'svelte';

  // Direction A callout. Renders the `.aside` recipe from arcade.css (2px ink
  // border, 7px rune left-bar, rune-soft fill, `.at` mono uppercase label). The
  // legacy content authored `<Aside title=… data-variant="wyrd|not">`; that
  // still works — `type`/`data-variant` are accepted but Direction A uses one
  // uniform aside treatment, so they only drive the default label text.
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
  const label = $derived(title ?? resolvedType);
</script>

<div class="aside" {...rest}>
  <div class="at">{label}</div>
  {@render children?.()}
</div>
