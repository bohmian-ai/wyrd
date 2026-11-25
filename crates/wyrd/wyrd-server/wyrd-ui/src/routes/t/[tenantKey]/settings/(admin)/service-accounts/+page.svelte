<!--
  Settings → Service accounts: non-human accounts that sign in with an API
  key. Counts and rows are the server's; a row opens the account's drawer.
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { replaceState } from '$app/navigation';
  import { page } from '$app/state';
  import { ICONS } from '$lib/icons';
  import Icon from '$lib/Icon.svelte';
  import Overlay from '$lib/Overlay.svelte';
  import Pill from '$lib/Pill.svelte';
  import AccountDrawer, { familyOf, labelOf } from '../AccountDrawer.svelte';
  import Outcome from '../Outcome.svelte';
  import Refusal from '../Refusal.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();

  const services = $derived(data.services);
  const params = $derived(page.url.searchParams);
  const base = $derived(page.url.pathname);
  const selectedId = $derived(params.get('person'));
  const summary = $derived(services?.principals.find((one) => one.principal_id === selectedId));
  const action = (name: string) => {
    const kept = [...params].filter(([key]) => !key.startsWith('/'));
    return `?/${name}${kept.length ? `&${new URLSearchParams(kept)}` : ''}`;
  };
</script>

<svelte:head><title>Service accounts · {data.tenant.name} · Wyrd</title></svelte:head>
<header class="pagehead">
  <div><h1>Service accounts</h1><p>Non-human accounts for CI, schedulers and agents. Each signs in with an API key.</p></div>
  {#if services}<div class="actions"><a class="app-control primary lg" href="{base}?drawer=new" data-sveltekit-noscroll><Icon d={ICONS.plus} />New service account</a></div>{/if}
</header>

<Outcome {form} />

{#if !services}
  <Refusal problem={data.problem} action="Administer service accounts" />
{:else}
  <div class="stats">
    <div class="stat"><span class="v">{services.counts.active + services.counts.suspended}</span><span class="k">Accounts</span></div>
    <div class="stat"><span class="v">{services.counts.suspended}</span><span class="k">Suspended</span></div>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling table must be reachable by keyboard -->
  <div class="tablewrap" role="region" aria-label="Service accounts" tabindex="0">
    {#if services.principals.length}
      <table>
        <thead><tr><th>Name</th><th>Restriction</th><th>Roles</th><th>Status</th></tr></thead>
        <tbody>
          {#each services.principals as account (account.principal_id)}
            <tr class="link">
              <td><a href="{base}?person={encodeURIComponent(account.principal_id)}" style="text-decoration:none" data-sveltekit-noscroll><b>{labelOf(account)}</b></a></td>
              <td>{#if familyOf(account.workload_restriction)}<span class="role">{familyOf(account.workload_restriction)}</span>{:else}<span class="muted">None</span>{/if}</td>
              <td>{#each account.roles as grant (grant.role + JSON.stringify(grant.scope))}<span class="role">{grant.role}</span>{:else}<span class="muted">None</span>{/each}</td>
              <td>{#if account.status === 'active'}<Pill label="Active" />{:else}<Pill tone="off" label="Suspended" />{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <div class="empty"><b>No service accounts yet</b><p>Create one for each piece of automation, so each has its own key and access.</p></div>
    {/if}
  </div>
  {#if services.next}<p style="margin-top:12px"><a href="{base}?after={encodeURIComponent(services.next)}">More service accounts</a></p>{/if}

  {#if params.get('drawer') === 'new'}
    <Overlay kind="drawer" title="New service account" close={base}>
      <form id="new-service" class="stack" method="POST" action={action('createService')} aria-label="Create a Service account" use:enhance={() => async ({ result, update }) => {
        await update();
        if (result.type === 'success') replaceState(base, {});
      }}>
        <div class="field"><label for="sa-name">Name</label><input id="sa-name" class="app-input mono" name="name" required maxlength="128" autocomplete="off" /></div>
        <div class="field"><label for="sa-description">Description</label><input id="sa-description" class="app-input" name="description" maxlength="256" autocomplete="off" /></div>
        <div class="field">
          <label for="sa-family">Runtime family</label>
          <input id="sa-family" class="app-input mono" name="family" autocomplete="off" placeholder="Service Card name, e.g. checkout" />
          <span class="hint">Optional. The Service Card family this account runs as, not a release: the same account and key serve every release.</span>
        </div>
        <p class="muted" style="font-size:12px">The account starts with no roles. Its first key is shown once after you create it.</p>
      </form>
      {#snippet footer()}
        <a class="app-control" href={base} data-sveltekit-noscroll>Cancel</a>
        <button class="app-control primary" type="submit" form="new-service">Create service account</button>
      {/snippet}
    </Overlay>
  {:else if services.selected}
    <AccountDrawer {summary} selected={services.selected} roles={services.roles} close={base} {action} />
  {/if}
{/if}

<style>
  .stack {
    display: grid;
    gap: 18px;
  }
</style>
