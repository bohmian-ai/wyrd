<!--
  Settings → People: who can sign in to the workspace, how they sign in and
  what they can do. Counts, rows and invitations are the server's; the page
  words them, keeps filters and drawers in the URL and posts access actions.
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { replaceState } from '$app/navigation';
  import { page } from '$app/state';
  import type { CardTypes } from 'wyrd-ai';
  import { fmtInstant } from 'wyrd-ai-ui';
  import { scopeLabel } from '$lib/access';
  import { ICONS } from '$lib/icons';
  import Icon from '$lib/Icon.svelte';
  import Mark from '$lib/Mark.svelte';
  import Overlay from '$lib/Overlay.svelte';
  import Pill from '$lib/Pill.svelte';
  import AccountDrawer, { METHODS, initialsOf, labelOf } from '../AccountDrawer.svelte';
  import GrantFields from '../GrantFields.svelte';
  import Outcome from '../Outcome.svelte';
  import Refusal from '../Refusal.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();

  const people = $derived(data.people);
  const params = $derived(page.url.searchParams);
  const filter = $derived(params.get('filter') === 'invited' ? 'invited' : 'members');
  const role = $derived(params.get('role'));
  const you = $derived(page.data.session?.subject.id);
  const invited = $derived(people?.invitations.length ?? 0);
  const undelivered = $derived(people?.invitations.filter((one) => one.delivery === 'failed').length ?? 0);
  const selectedId = $derived(params.get('person'));
  const summary = $derived(people?.principals.find((one) => one.principal_id === selectedId));

  /** This page with `changes` applied to its query; `null` removes a key. Drawers and paging never carry over. */
  function href(changes: Record<string, string | null>) {
    const next = new URLSearchParams(params);
    for (const key of ['drawer', 'person', 'after']) next.delete(key);
    for (const [key, value] of Object.entries(changes)) {
      if (value === null) next.delete(key);
      else next.set(key, value);
    }
    const query = next.toString();
    return `${page.url.pathname}${query ? `?${query}` : ''}`;
  }
  /** A form action that keeps the page's query when the form posts without JavaScript. */
  const action = (name: string) => {
    const kept = [...params].filter(([key]) => !key.startsWith('/'));
    return `?/${name}${kept.length ? `&${new URLSearchParams(kept)}` : ''}`;
  };
  const scoped = (assignment: { role: string; scope: CardTypes.PermissionScope }) => assignment.role + (assignment.scope === 'all' ? '' : ' · scoped');
</script>

<svelte:head><title>People · {data.tenant.name} · Wyrd</title></svelte:head>
<header class="pagehead">
  <div>
    <h1>People</h1>
    <p>Who can sign in to {data.tenant.name}, how they sign in, and what they can do.</p>
  </div>
  {#if people}<div class="actions"><a class="app-control primary lg" href={href({ drawer: 'invite' })} data-sveltekit-noscroll><Icon d={ICONS.plus} />Invite people</a></div>{/if}
</header>

<Outcome {form} />

{#if !people}
  <Refusal problem={data.problem} action="Administer people" />
{:else}
  <div class="stats">
    <a class="stat" href={href({ filter: null })}><span class="v">{people.counts.active}</span><span class="k">Active</span></a>
    <a class="stat" href={href({ filter: 'invited' })}><span class="v">{invited}</span><span class="k">Invited · pending</span></a>
    {#if undelivered}<a class="stat warn" href={href({ filter: 'invited' })}><span class="v">{undelivered}</span><span class="k">Invitation not delivered</span></a>{/if}
    <div class="stat"><span class="v">{people.counts.suspended}</span><span class="k">Suspended</span></div>
  </div>

  <div class="toolbar">
    <div class="seg" role="group" aria-label="Filter">
      <a href={href({ filter: null })} aria-current={filter === 'members' ? 'true' : undefined}>Members</a>
      <a href={href({ filter: 'invited' })} aria-current={filter === 'invited' ? 'true' : undefined}>Invited ({invited})</a>
    </div>
    {#if role}<a class="app-control" href={href({ role: null })} aria-label="Stop filtering by role {role}">Role: {role} ✕</a>{/if}
    <form class="find" method="GET" role="search">
      {#if filter === 'invited'}<input type="hidden" name="filter" value="invited" />{/if}
      {#if role}<input type="hidden" name="role" value={role} />{/if}
      <input class="app-input" name="q" value={params.get('q') ?? ''} placeholder="Exact email or username" aria-label="Find people by exact email or username" />
    </form>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling table must be reachable by keyboard -->
  <div class="tablewrap" role="region" aria-label={filter === 'invited' ? 'Pending invitations' : 'People'} tabindex="0">
    {#if filter === 'invited'}
      {#if people.invitations.length}
        <table>
          <thead><tr><th>Email</th><th>Will get</th><th>Delivery</th><th>Expires</th><th class="right"></th></tr></thead>
          <tbody>
            {#each people.invitations as one (one.id)}
              <tr>
                <td>{one.recipient}</td>
                <td>{#each one.assignments as grant (grant.role + JSON.stringify(grant.scope))}<span class="role" title={scopeLabel(grant.scope)}>{scoped(grant)}</span>{/each}</td>
                <td>{#if one.delivery === 'failed'}<Pill tone="warn" label="Not delivered" />{:else if one.delivery === 'accepted'}<Pill tone="info" label="Emailed" />{:else}<Pill tone="off" label="Not emailed" />{/if}</td>
                <td class="muted">{fmtInstant(Date.parse(one.expires_at))}</td>
                <td class="right" style="white-space:nowrap">
                  <form style="display:inline" method="POST" action={action('resendInvitation')} use:enhance>
                    <input type="hidden" name="invitationId" value={one.id} />
                    <button class="app-control" type="submit" aria-label="Resend invitation to {one.recipient}">Resend</button>
                  </form>
                  <form style="display:inline" method="POST" action={action('revokeInvitation')} use:enhance>
                    <input type="hidden" name="invitationId" value={one.id} />
                    <button class="app-control" type="submit" aria-label="Revoke invitation to {one.recipient}">Revoke</button>
                  </form>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <div class="empty"><b>No pending invitations</b><p>Invite people to send each one a single-use link.</p></div>
      {/if}
    {:else if people.principals.length}
      <table>
        <thead><tr><th>Person</th><th>Signs in with</th><th>Roles</th><th>Status</th></tr></thead>
        <tbody>
          {#each people.principals as person (person.principal_id)}
            {@const open = href({ person: person.principal_id })}
            <tr class="link">
              <td>
                <a class="person" href={open} style="text-decoration:none" data-sveltekit-noscroll>
                  <span class="avatar" aria-hidden="true">{initialsOf(person)}</span>
                  <span><b>{labelOf(person)}{person.principal_id === you ? ' (you)' : ''}</b>{#if person.email}<span>{person.email}</span>{/if}</span>
                </a>
              </td>
              <td>{#if person.authentication}<span class="method"><Mark method={person.authentication} />{METHODS[person.authentication]}</span>{/if}</td>
              <td>{#each person.roles as grant (grant.role + JSON.stringify(grant.scope))}<span class="role" title={scopeLabel(grant.scope)}>{scoped(grant)}</span>{:else}<span class="muted">None</span>{/each}</td>
              <td>{#if person.status === 'active'}<Pill label="Active" />{:else}<Pill tone="off" label="Suspended" />{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <div class="empty"><b>No one matches</b><p>{params.get('q') || role ? 'Clear the filter to see everyone.' : 'Invite people to give them access.'}</p></div>
    {/if}
  </div>
  {#if people.next && filter === 'members'}<p style="margin-top:12px"><a href={href({ after: people.next })}>More people</a></p>{/if}

  {#if params.get('drawer') === 'invite'}
    <Overlay kind="drawer" title="Invite people" close={href({})}>
      <form id="invite" class="invite" method="POST" action={action('invite')} aria-label="Invite people" use:enhance={() => async ({ result, update }) => {
          await update();
          // The links show once, beside the closed drawer; a reload never brings them back.
          if (result.type === 'success') replaceState(href({}), {});
        }}>
        <div class="field">
          <label for="emails">Email addresses</label>
          <textarea id="emails" class="app-input" name="recipients" rows="4" required autocomplete="off" placeholder="dana@example.test, omar@example.test"></textarea>
          <span class="hint">Separate with commas or new lines. Each person gets their own link, valid for 24 hours.</span>
        </div>
        <GrantFields roles={people.roles.filter((one) => one.name !== 'workload')} pick="choices" />
        <p class="muted" style="font-size:12px">They choose how to sign in when they accept. Add more roles from their profile later.</p>
      </form>
      {#snippet footer()}
        <a class="app-control" href={href({})} data-sveltekit-noscroll>Cancel</a>
        <button class="app-control primary" type="submit" form="invite">Send invitations</button>
      {/snippet}
    </Overlay>
  {:else if people.selected}
    <AccountDrawer {summary} selected={people.selected} roles={people.roles} close={href({})} {action} />
  {/if}
{/if}

<style>
  .find {
    margin-left: auto;
  }
  .find .app-input {
    width: 240px;
  }
  .invite {
    display: grid;
    gap: 18px;
  }
  @media (max-width: 900px) {
    .find {
      width: 100%;
      margin-left: 0;
    }
    .find .app-input {
      width: 100%;
    }
  }
</style>
