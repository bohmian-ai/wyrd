<!-- The page's place in the tenant: each ancestor a link, the page itself last. -->
<script lang="ts">
  let { items }: { items: readonly { readonly label: string; readonly href?: string }[] } = $props();
</script>

<nav class="bar" aria-label="Breadcrumb">
  {#each items as item, index (index)}
    {#if index > 0}<span aria-hidden="true">/</span>{/if}
    {#if item.href}<a href={item.href}>{item.label}</a>{:else if index === items.length - 1}<span class="here" aria-current="page">{item.label}</span>{:else}<span>{item.label}</span>{/if}
  {/each}
</nav>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
    padding: 14px 24px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  a {
    color: var(--muted);
  }
  .here {
    color: var(--text);
    overflow-wrap: anywhere;
  }
  @media (max-width: 767px) {
    .bar {
      padding: 12px 16px;
    }
  }
</style>
