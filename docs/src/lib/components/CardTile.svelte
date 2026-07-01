<script lang="ts">
  import { base } from '$app/paths';

  // A single card-kind tile. Self-contained visual recipe (2px border, 5px
  // semantic left-bar, hard-offset shadow) in the scoped style block below, so it
  // stands alone in prose or as a standalone call-to-action.
  let {
    href,
    label,
    kind,
    status = 'shipped'
  }: {
    href: string;
    label: string;
    kind?: string;
    status?: 'shipped' | 'spec';
  } = $props();

  const resolvedHref = $derived(href.startsWith('/') ? `${base}${href}` : href);
</script>

<a class="wyrd-card-tile" href={resolvedHref} data-kind={kind}>
  {label}
  {#if status === 'spec'}
    <small>spec only</small>
  {/if}
</a>

<style>
  .wyrd-card-tile {
    display: inline-flex;
    flex-direction: column;
    gap: 4px;
    font-family: var(--font-mono);
    font-weight: 700;
    font-size: 0.85rem;
    text-decoration: none;
    color: var(--text);
    background: var(--surface);
    border: 2px solid var(--border);
    border-left-width: 5px;
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    padding: 10px 13px;
    transition:
      transform 0.08s,
      box-shadow 0.08s;
  }
  .wyrd-card-tile:hover {
    transform: translate(-1px, -1px);
    box-shadow: 5px 5px 0 0 var(--shadow);
  }
  small {
    font-family: var(--font-mono);
    font-weight: 700;
    font-size: 0.6rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  /* Semantic left-bar — same kind→color mapping as the grid. */
  .wyrd-card-tile[data-kind='agent'],
  .wyrd-card-tile[data-kind='data'],
  .wyrd-card-tile[data-kind='source'],
  .wyrd-card-tile[data-kind='trigger'],
  .wyrd-card-tile[data-kind='workflow'] {
    border-left-color: var(--client-bar);
  }
  .wyrd-card-tile[data-kind='service'],
  .wyrd-card-tile[data-kind='eval'],
  .wyrd-card-tile[data-kind='experiment'],
  .wyrd-card-tile[data-kind='mcp'] {
    border-left-color: var(--server-bar);
  }
  .wyrd-card-tile[data-kind='policy'],
  .wyrd-card-tile[data-kind='audit'],
  .wyrd-card-tile[data-kind='operator'] {
    border-left-color: var(--control-bar);
  }
  .wyrd-card-tile[data-kind='model'],
  .wyrd-card-tile[data-kind='prompt'],
  .wyrd-card-tile[data-kind='artifact'],
  .wyrd-card-tile[data-kind='drift'] {
    border-left-color: var(--rune-strong);
  }
</style>
