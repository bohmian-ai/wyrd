<script lang="ts">
  import type { FieldFacet } from '$lib/features/observe/core/types';

  // The fields side panel: server-projected field facets with match counts.
  // Clicking a value filters to it; clicking the active value clears that field.
  // Hrefs come from the page so filtering stays plain URL navigation — the
  // panel authors links, never client state.
  let {
    fields,
    makeHref
  }: {
    fields: FieldFacet[];
    makeHref: (field: string, value: string, active: boolean) => string;
  } = $props();
  let collapsed = $state(false);
</script>

<aside class="wy-fields" class:collapsed aria-label="Filter fields">
  <button
    class="head"
    type="button"
    onclick={() => (collapsed = !collapsed)}
    aria-expanded={!collapsed}
  >
    {collapsed ? '»' : '« FIELDS'}
  </button>
  {#if !collapsed}
    {#each fields as field (field.name)}
      <details open>
        <summary>{field.name}</summary>
        <ul>
          {#each field.values as v (v.value)}
            <li>
              <a
                href={makeHref(field.name, v.value, v.active)}
                data-sveltekit-noscroll
                class:active={v.active}
                title={v.active ? `clear ${field.name}` : `filter ${field.name} = ${v.value}`}
              >
                <span class="val">{v.value}</span><span class="cnt">{v.count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </details>
    {/each}
  {/if}
</aside>

<style>
  .wy-fields {
    width: 176px;
    flex: 0 0 auto;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 3px 3px 0 0 var(--shadow);
    padding: 4px;
    align-self: flex-start;
    font-family: var(--fm);
  }
  .wy-fields.collapsed {
    width: auto;
  }
  .head {
    display: block;
    width: 100%;
    text-align: left;
    padding: 5px 7px;
    border: 0;
    background: none;
    color: var(--muted);
    font: 700 9px var(--fm);
    letter-spacing: 0.6px;
    cursor: pointer;
  }
  .head:hover {
    color: var(--text);
  }
  summary {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--muted);
    padding: 5px 7px;
    cursor: pointer;
    border-top: 2px dashed var(--border);
  }
  ul {
    list-style: none;
    margin: 0 0 4px;
    padding: 0;
  }
  li a {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 3px 7px;
    font-size: 11px;
    color: var(--text);
    text-decoration: none;
    border-radius: var(--r);
    border: 2px solid transparent;
  }
  li a:hover {
    background: var(--surface-2);
  }
  li a.active {
    background: var(--brand-soft);
    border-color: var(--border);
  }
  .val {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cnt {
    color: var(--muted);
    font-size: 10px;
  }
</style>
