<!--
  Settings → Roles: what each role allows, with the server's description and
  member count. Built-in roles are read-only; custom roles are created, edited
  and deleted through the existing role actions.
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { replaceState } from '$app/navigation';
  import { page } from '$app/state';
  import type { SubmitFunction } from '@sveltejs/kit';
  import type { CardTypes } from 'wyrd-ai';
  import { OPERATIONS, operationName } from '$lib/access';
  import { ICONS } from '$lib/icons';
  import Icon from '$lib/Icon.svelte';
  import Overlay from '$lib/Overlay.svelte';
  import Outcome from '../Outcome.svelte';
  import Refusal from '../Refusal.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();

  const base = $derived(page.url.pathname);
  const params = $derived(page.url.searchParams);
  const open = $derived(params.get('open'));
  const editing = $derived(data.roles?.find((role) => !role.builtin && role.name === params.get('edit')));
  const deleting = $derived(data.roles?.find((role) => !role.builtin && role.name === params.get('delete')));
  const people = $derived(`/t/${encodeURIComponent(data.tenant.key)}/settings/people`);
  const held = (role: CardTypes.RoleView) => role.operations.map(operationName);
  const count = (role: CardTypes.RoleView) => (held(role).includes('*') ? 'All operations' : `${role.operations.length} operation${role.operations.length === 1 ? '' : 's'}`);
  /** Close the overlay without navigating once the server accepted its form. */
  const closing: SubmitFunction = () => async ({ result, update }) => {
    await update();
    if (result.type === 'success') replaceState(base, {});
  };
</script>

{#snippet roleForm(id: string, role: CardTypes.RoleView | undefined)}
  <form {id} class="stack" method="POST" action="?/putRole" aria-label={role ? `Edit role ${role.name}` : 'Create a custom role'} use:enhance={closing}>
    {#if role}
      <input type="hidden" name="role" value={role.name} />
      {#if role.member_count}<p class="note warn" style="margin:0"><span class="g" aria-hidden="true">!</span><span class="grow">Saving changes what {role.member_count} {role.member_count === 1 ? 'account' : 'accounts'} can do.</span></p>{/if}
    {:else}
      <div class="field">
        <label for="role-name">Name</label>
        <input id="role-name" class="app-input mono" name="role" required maxlength="64" pattern="[a-z0-9_]+" title="Lowercase letters, digits and underscores" autocomplete="off" />
        <span class="hint">Lowercase letters, digits and underscores.</span>
      </div>
    {/if}
    <div class="field">
      <label for="{id}-description">Description</label>
      <input id="{id}-description" class="app-input" name="description" maxlength="200" autocomplete="off" value={role?.description ?? ''} />
      <span class="hint">One line saying what the role is for. Optional.</span>
    </div>
    <div class="field">
      <span class="legend">Operations</span>
      <div class="operations">
        {#each Object.entries(OPERATIONS) as [resource, actions] (resource)}
          <fieldset>
            <legend class="mono">{resource}</legend>
            {#each actions as act (act)}
              <label class="check"><input type="checkbox" name="operation" value={`${resource}:${act}`} checked={role ? held(role).includes(`${resource}:${act}`) : false} /> {act}</label>
            {/each}
          </fieldset>
        {/each}
      </div>
      <span class="hint">A role needs at least one operation. It applies only at the scope where it is granted.</span>
    </div>
  </form>
{/snippet}

<svelte:head><title>Roles · {data.tenant.name} · Wyrd</title></svelte:head>
<header class="pagehead">
  <div><h1>Roles</h1><p>What each role allows. A role applies wherever it is granted: the whole workspace, or one Card or repository.</p></div>
  {#if data.roles}<div class="actions"><a class="app-control primary lg" href="{base}?drawer=new" data-sveltekit-noscroll><Icon d={ICONS.plus} />New role</a></div>{/if}
</header>

<Outcome {form} />

{#if !data.roles}
  <Refusal problem={data.problem} action="Administer roles" />
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling table must be reachable by keyboard -->
  <div class="tablewrap" role="region" aria-label="Roles" tabindex="0">
    <table>
      <thead><tr><th>Role</th><th>Allows</th><th class="right">Members</th><th class="right"></th></tr></thead>
      <tbody>
        {#each data.roles as role (role.name)}
          <tr data-role={role.name}>
            <td style="white-space:nowrap"><b>{role.name}</b> <span class="muted" style="font-size:12px">{role.builtin ? 'built-in' : 'custom'}</span></td>
            <td>
              {#if role.description}{role.description}{/if}
              {#if open === role.name}
                <div style="margin-top:8px">{#each held(role) as op (op)}<span class="role mono">{op}</span>{/each}</div>
              {/if}
            </td>
            <td class="right">{#if role.member_count}<a href="{people}?role={encodeURIComponent(role.name)}" aria-label="{role.member_count} members with {role.name}">{role.member_count}</a>{:else}<span class="muted">0</span>{/if}</td>
            <td class="right" style="white-space:nowrap">
              <a class="app-control" href={open === role.name ? base : `${base}?open=${encodeURIComponent(role.name)}`} aria-expanded={open === role.name} data-sveltekit-noscroll>{open === role.name ? 'Hide' : count(role)}</a>
              {#if !role.builtin}
                <a class="app-control" href="{base}?edit={encodeURIComponent(role.name)}" aria-label="Edit {role.name}" data-sveltekit-noscroll>Edit</a>
                <a class="app-control" href="{base}?delete={encodeURIComponent(role.name)}" aria-label="Delete {role.name}" data-sveltekit-noscroll>Delete</a>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  {#if params.get('drawer') === 'new'}
    <Overlay kind="drawer" title="New role" close={base}>
      {@render roleForm('new-role', undefined)}
      {#snippet footer()}
        <a class="app-control" href={base} data-sveltekit-noscroll>Cancel</a>
        <button class="app-control primary" type="submit" form="new-role">Create role</button>
      {/snippet}
    </Overlay>
  {:else if editing}
    <Overlay kind="drawer" title="Edit {editing.name}" close={base}>
      {@render roleForm('edit-role', editing)}
      {#snippet footer()}
        <a class="app-control" href={base} data-sveltekit-noscroll>Cancel</a>
        <button class="app-control primary" type="submit" form="edit-role">Save {editing.name}</button>
      {/snippet}
    </Overlay>
  {:else if deleting}
    <Overlay kind="dialog" title="Delete {deleting.name}?" close={base}>
      {#if deleting.member_count}
        <p>{deleting.member_count} {deleting.member_count === 1 ? 'account holds' : 'accounts hold'} this role. Remove it from them first, then delete it.</p>
      {:else}
        <p>No one holds this role. Deleting it cannot be undone.</p>
      {/if}
      {#snippet footer()}
        <a class="app-control" href={base} data-sveltekit-noscroll>Cancel</a>
        <form method="POST" action="?/deleteRole" use:enhance={closing}>
          <input type="hidden" name="role" value={deleting.name} />
          <button class="app-control danger fill" type="submit" disabled={deleting.member_count > 0}>Delete role</button>
        </form>
      {/snippet}
    </Overlay>
  {/if}
{/if}

<style>
  .stack {
    display: grid;
    gap: 18px;
  }
  .legend {
    font-size: 13px;
    font-weight: var(--weight-strong);
  }
  .operations {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 10px;
  }
  fieldset {
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
  }
  legend {
    padding: 0 4px;
    color: var(--muted);
  }
  .operations .check {
    align-items: center;
  }
</style>
