<script lang="ts">
  // At-a-glance summary block for a generated card reference page: kind + purpose,
  // the required fields, and the optional-field count. The generator passes data
  // (kind slug, title, purpose, required names, optional count); this component
  // owns the presentation via the `.wyrd-defs` definition-list recipe in
  // styles/docs.css, so it stays token-driven.
  let {
    kind,
    title,
    purpose,
    required,
    optionalCount
  }: {
    kind: string;
    title: string;
    purpose: string;
    required: string[];
    optionalCount: number;
  } = $props();

  // Cap the inline required list so the block stays scannable; the full field
  // set is the generated table below it.
  const shown = $derived(required.slice(0, 4));
</script>

<dl class="wyrd-defs">
  <dt data-kind={kind}>{title}</dt>
  <dd>{purpose}</dd>
  <dt>Required</dt>
  <dd>
    {#if shown.length}
      {#each shown as name, i (name)}<code>{name}</code>{#if i < shown.length - 1}, {/if}{/each}
    {:else}none required{/if}
  </dd>
  <dt>Optional</dt>
  <dd>{optionalCount} additional spec fields — see table below.</dd>
</dl>
