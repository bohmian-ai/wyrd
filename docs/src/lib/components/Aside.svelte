<script lang="ts">
  import type { Snippet } from 'svelte';

  // Brutalist callout. Uses the brand-correct `.wyrd-aside` recipe already
  // defined in src/styles/wyrd.css (2px border, 5px radius, hard-offset shadow,
  // semantic left-bar by type — all token-driven, no hardcoded hex). The legacy
  // content authored `<Aside title=… data-variant="wyrd|not">`; `data-variant`
  // passes through (rest props) and, when no explicit `type` is given, picks the
  // accent: `wyrd` → note (rune), `not` → caution (warn).
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
</script>

<aside class={`wyrd-aside wyrd-aside--${resolvedType}`} {...rest}>
  {#if title}
    <p class="wyrd-aside__title">{title}</p>
  {/if}
  <div class="wyrd-aside__content">
    {@render children?.()}
  </div>
</aside>

<style>
  /* Padding only — color/border/radius/shadow come from the global recipe. */
  .wyrd-aside {
    padding: 14px 16px;
    margin: 1.5rem 0;
  }
  .wyrd-aside__title {
    margin: 0 0 0.5rem;
    font-size: 0.78rem;
  }
  .wyrd-aside__content :global(:first-child) {
    margin-top: 0;
  }
  .wyrd-aside__content :global(:last-child) {
    margin-bottom: 0;
  }
</style>
