<script lang="ts">
  import type { SessionMetadata } from '$lib/views';
  let { session }: { session: SessionMetadata } = $props();
  let search = $state('');
  let matches = $derived(
    session.tenants.filter((tenant) =>
      `${tenant.name} ${tenant.key}`.toLowerCase().includes(search.toLowerCase())
    )
  );
  const id = $props.id();
</script>

<div class="chooser">
  <label for={id}>Search tenants</label>
  <input id={id} class="app-input" placeholder="⌕ filter tenants" type="search" bind:value={search} />
  <form method="POST" action="/?/switch">
    <input type="hidden" name="csrf" value={session.csrf} />
    {#each matches as tenant (tenant.key)}
      <button class="app-control" type="submit" name="tenantKey" value={tenant.key}>{tenant.name}</button>
    {:else}
      <p role="status">No matching tenants.</p>
    {/each}
  </form>
</div>

<style>
  .chooser {
    display: grid;
    gap: 10px;
    padding-top: 0;
  }
  label {
    font: 500 12px var(--font-sans);
  }
  form {
    display: grid;
    gap: 8px;
  }
  button {
    text-align: left;
    box-shadow: none;
    border: 0;
    border-bottom: 2px solid var(--surface-2);
    border-radius: 0;
    font-weight: 400;
    padding: 12px;
  }
  button:hover,
  button:focus-visible {
    background: var(--brand-soft);
    box-shadow: inset 4px 0 0 var(--brand-strong);
  }
  input {
    background: var(--surface-2);
    border-color: transparent;
    font-weight: 400;
  }
  label {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
